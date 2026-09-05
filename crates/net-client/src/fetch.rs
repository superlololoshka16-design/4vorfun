use crate::engine::EngineSet;
use compact_str::CompactString;
use futures_util::StreamExt;
use parser_pipeline::{Flow, PageData, StreamPipeline};
use session_state::Session;
use smallvec::SmallVec;
use std::sync::Arc;
use std::time::Instant;
use thiserror::Error;
use wreq::header::{HeaderMap, HeaderName, HeaderValue, SET_COOKIE};

#[derive(Debug, Error)]
pub enum NetError {
    #[error("bad url")]
    Url,
    #[error("transport: {0}")]
    Transport(#[from] wreq::Error),
    #[error("status {0}")]
    Status(u16),
    #[error("pipe: {0}")]
    Pipe(#[from] parser_pipeline::PipeError),
    #[error("bad telemetry payload")]
    Payload,
}

pub struct Fetched {
    pub status: u16,
    pub uri: CompactString,
    pub page: Arc<PageData>,
    pub bytes_in: u64,
    pub elapsed_ms: u64,
}

fn cookie_header(jar: &session_state::CookieJar) -> Option<HeaderValue> {
    let mut buf: SmallVec<[u8; 256]> = SmallVec::new();
    jar.header_into(&mut buf);
    if buf.is_empty() {
        return None;
    }
    HeaderValue::from_bytes(&buf).ok()
}

pub async fn push_telemetry(
    engines: &EngineSet,
    slot: usize,
    session: &mut Session,
    route: &parser_pipeline::TelemetryRoute,
    blob: &[u8],
) -> Result<u16, NetError> {
    let client = engines.client_for(slot);
    let endpoint = resolve_endpoint(&session.origin, route.endpoint.as_str());
    let mut req = match route.transport {
        parser_pipeline::Transport::FormField => {
            let value = core_utils::base64::STANDARD.encode_to_string(blob);
            client.post(endpoint.as_str()).form(&[(route.field.as_str(), value)])
        }
        parser_pipeline::Transport::CustomHeader => {
            let name = HeaderName::from_bytes(route.field.as_bytes()).map_err(|_| NetError::Payload)?;
            let value = HeaderValue::from_bytes(blob).map_err(|_| NetError::Payload)?;
            client.post(endpoint.as_str()).header(name, value)
        }
        parser_pipeline::Transport::CdnPost => client
            .post(endpoint.as_str())
            .header(wreq::header::CONTENT_TYPE, "application/octet-stream")
            .body(blob.to_vec()),
    };
    if let Some(cv) = cookie_header(&session.jar) {
        let mut hm = HeaderMap::with_capacity(1);
        hm.insert(HeaderName::from_static("cookie"), cv);
        req = req.headers(hm);
    }
    let resp = req.send().await?;
    let status = resp.status().as_u16();
    for v in resp.headers().get_all(SET_COOKIE).iter() {
        if let Ok(line) = v.to_str() {
            session.jar.ingest(line);
        }
    }
    session.touch();
    Ok(status)
}

fn resolve_endpoint(origin: &str, endpoint: &str) -> CompactString {
    if endpoint.starts_with("http://") || endpoint.starts_with("https://") {
        return CompactString::new(endpoint);
    }
    let base = match origin.find("://") {
        Some(s) => {
            let after = &origin[s + 3..];
            let cut = after.find('/').map(|i| s + 3 + i).unwrap_or(origin.len());
            &origin[..cut]
        }
        None => origin,
    };
    CompactString::from(format!("{base}{endpoint}"))
}

pub async fn fetch_page(
    engines: &EngineSet,
    slot: usize,
    session: &mut Session,
    url: &str,
) -> Result<Fetched, NetError> {
    fetch_page_sel(engines, slot, session, url, &[]).await
}

pub async fn fetch_page_sel(
    engines: &EngineSet,
    slot: usize,
    session: &mut Session,
    url: &str,
    selectors: &[(String, String)],
) -> Result<Fetched, NetError> {
    let start = Instant::now();
    let client = engines.client_for(slot);
    let mut req = client.get(url);
    if let Some(cv) = cookie_header(&session.jar) {
        let mut hm = HeaderMap::with_capacity(1);
        hm.insert(HeaderName::from_static("cookie"), cv);
        req = req.headers(hm);
    }
    let resp = req.send().await?;
    let status = resp.status().as_u16();
    let uri = CompactString::from(resp.uri().to_string());
    for v in resp.headers().get_all(SET_COOKIE).iter() {
        if let Ok(line) = v.to_str() {
            session.jar.ingest(line);
        }
    }
    session.touch();
    let mut pipeline = if selectors.is_empty() {
        StreamPipeline::new(Default::default())
    } else {
        StreamPipeline::with_selectors(Default::default(), selectors)
    };
    let mut stream = resp.bytes_stream();
    let mut bytes_in: u64 = 0;
    let mut stopped = false;
    let mut head_buf: SmallVec<[u8; 2048]> = SmallVec::new();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(NetError::Transport)?;
        bytes_in += chunk.len() as u64;
        if head_buf.len() < 2048 {
            let take = chunk.len().min(2048 - head_buf.len());
            head_buf.extend_from_slice(&chunk[..take]);
        }
        if pipeline.push(&chunk)? == Flow::Stop {
            stopped = true;
            break;
        }
    }
    let mut page = pipeline.finish()?;
    page.html_head = Some(head_buf);
    if stopped {
        tracing::debug!(target: "net", "byte brake hit after {bytes_in} bytes");
    }
    Ok(Fetched {
        status,
        uri,
        page: Arc::new(page),
        bytes_in,
        elapsed_ms: start.elapsed().as_millis() as u64,
    })
}
