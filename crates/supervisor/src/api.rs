use std::sync::Arc;
use std::sync::OnceLock;
use std::sync::atomic::{AtomicU8, AtomicU64, Ordering};
use std::time::{Duration, Instant};

use axum::extract::State;
use axum::http::StatusCode;
use axum::routing::{get, post};
use axum::{Json, Router};
use net_client::{EngineSet, Fetched, fetch_page_sel, push_telemetry, reslot};
use parser_pipeline::{StreamPipeline, validate_selectors};
use core_utils::xxh3;
use payload_gen::input::{
    Persona, RawEvent, TabSession, TelemetryBatcher, BATCH_CAP, BATCH_INTERVAL_US, events_bytes,
};
use runtime_exec::{ExecReq, ProfileSnap, WorkerPool};
use scc::HashMap;
use serde::{Deserialize, Serialize};
use session_state::{Profile, Session};
use smallvec::SmallVec;
use tokio::sync::{Semaphore, mpsc};

use crate::stats::StatsRef;

pub type TaskId = u64;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum TaskKind {
    HtmlFetch { url: String, method: Option<String> },
    Extract { url: String, selectors: Vec<(String, String)> },
    FormSubmit {
        url: String,
        method: Option<String>,
        fields: Vec<(String, String)>,
        token_field: Option<String>,
        token: Option<String>,
    },
    Solve {
        kind: String,
        script: Option<String>,
        payload_b64: Option<String>,
        deadline_ms: Option<u64>,
        profile_id: Option<u64>,
    },
}

impl TaskKind {
    fn url(&self) -> &str {
        match self {
            TaskKind::HtmlFetch { url, .. }
            | TaskKind::Extract { url, .. }
            | TaskKind::FormSubmit { url, .. } => url,
            TaskKind::Solve { .. } => "",
        }
    }
}

const ST_QUEUED: u8 = 0;
const ST_PROCESSING: u8 = 1;
const ST_READY: u8 = 2;
const ST_FAILED: u8 = 3;

static NEXT_TASK: AtomicU64 = AtomicU64::new(1);

struct TaskRec {
    submitted: Instant,
    deadline: Instant,
    state: AtomicU8,
    result: OnceLock<Arc<serde_json::value::Value>>,
}

impl TaskRec {
    fn new(ttl: Duration) -> Self {
        let now = Instant::now();
        Self { submitted: now, deadline: now + ttl, state: AtomicU8::new(ST_QUEUED), result: OnceLock::new() }
    }

    fn alive(&self) -> bool {
        Instant::now() < self.deadline
    }

    fn outcome(&self) -> TaskOutcome {
        let now = Instant::now();
        match self.state.load(Ordering::Acquire) {
            ST_QUEUED | ST_PROCESSING => {
                if now >= self.deadline {
                    return TaskOutcome::Failed { error: "expired".into() };
                }
                TaskOutcome::Processing {
                    elapsed_ms: ms(self.submitted, now),
                    ttl_left_ms: ms(now, self.deadline),
                }
            }
            _ => match self.result.get() {
                Some(_) if now > self.deadline => TaskOutcome::Failed { error: "expired".into() },
                Some(v) => TaskOutcome::Ready { solution: (**v).clone() },
                None => TaskOutcome::Failed { error: "no result".into() },
            },
        }
    }
}

fn ms(a: Instant, b: Instant) -> u64 {
    b.saturating_duration_since(a).as_millis() as u64
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum TaskOutcome {
    Processing { elapsed_ms: u64, ttl_left_ms: u64 },
    Ready { solution: serde_json::value::Value },
    Failed { error: String },
}

struct Job {
    id: TaskId,
    kind: TaskKind,
}

pub struct AppState {
    registry: Arc<HashMap<TaskId, Arc<TaskRec>>>,
    engines: Arc<EngineSet>,
    profiles: Vec<Arc<Profile>>,
    pool: Arc<WorkerPool>,
    stats: StatsRef,
    timeout: Duration,
    asn: u32,
    tx: mpsc::Sender<Job>,
    sem: Arc<Semaphore>,
    monitor: Arc<parser_pipeline::VersionMonitor>,
}

impl AppState {
    pub fn spawn(
        workers: usize,
        engines: Arc<EngineSet>,
        profiles: Vec<Arc<Profile>>,
        pool: Arc<WorkerPool>,
        stats: StatsRef,
        timeout: Duration,
        asn: u32,
        monitor: parser_pipeline::VersionMonitor,
    ) -> Arc<Self> {
        let registry = Arc::new(HashMap::new());
        let (tx, rx) = mpsc::channel::<Job>(1024);
        let sem = Arc::new(Semaphore::new(workers.max(1)));
        let state = Arc::new(Self {
            registry,
            engines,
            profiles,
            pool,
            stats,
            timeout,
            asn,
            tx,
            sem,
            monitor: Arc::new(monitor),
        });

        let st = state.clone();
        let mut rx = rx;
        tokio::spawn(async move {
            while let Some(job) = rx.recv().await {
                let st = st.clone();
                let sem = st.sem.clone();
                tokio::spawn(async move {
                    let permit = sem.acquire_owned().await;
                    let Some(rec) = st.registry.read_sync(&job.id, |_, r| Arc::clone(r)) else { return };
                    rec.state.store(ST_PROCESSING, Ordering::Release);
                    let out = execute(&st, &job.kind).await;
                    drop(permit);
                    match out {
                        Ok(v) => {
                            let _ = rec.result.set(Arc::new(v));
                            rec.state.store(ST_READY, Ordering::Release);
                        }
                        Err(e) => {
                            let _ = rec.result.set(Arc::new(serde_json::json!({ "error": e })));
                            rec.state.store(ST_FAILED, Ordering::Release);
                        }
                    }
                });
            }
        });

        let reg = state.registry.clone();
        tokio::spawn(async move {
            loop {
                tokio::time::sleep(Duration::from_secs(10)).await;
                reg.retain_sync(|_, r| r.alive());
            }
        });

        let mon = state.monitor.clone();
        tokio::spawn(async move {
            loop {
                tokio::time::sleep(Duration::from_secs(300)).await;
                mon.for_each_hash(|url, hash| {
                    tracing::info!(url, hash, "challenge build hash");
                });
            }
        });

        state
    }

    pub async fn create_task(&self, kind: TaskKind) -> Result<TaskId, String> {
        let url = kind.url();
        if !url.is_empty() && !url.starts_with("http://") && !url.starts_with("https://") {
            return Err("bad url".into());
        }
        if let TaskKind::Extract { selectors, .. } = &kind {
            validate_selectors(selectors)?;
        }
        if self.registry.len() >= 4096 {
            return Err("queue full".into());
        }
        let id = NEXT_TASK.fetch_add(1, Ordering::Relaxed);
        let rec = Arc::new(TaskRec::new(self.timeout));
        let _ = self.registry.insert_async(id, rec).await;
        if self.tx.send(Job { id, kind }).await.is_err() {
            return Err("workers down".into());
        }
        Ok(id)
    }

    pub fn task_outcome(&self, id: TaskId) -> TaskOutcome {
        match self.registry.read_sync(&id, |_, r| r.outcome()) {
            Some(o) => o,
            None => TaskOutcome::Failed { error: "not found".into() },
        }
    }

    pub fn stats(&self) -> serde_json::value::Value {
        serde_json::json!({
            "tasksInRegistry": self.registry.len(),
            "fetches": self.stats.fetches(),
            "bytesIn": self.stats.bytes(),
            "scripts": self.stats.scripts(),
            "apiTouches": self.stats.touches(),
            "uptimeSecs": self.stats.uptime_secs(),
        })
    }
}

async fn execute(st: &AppState, kind: &TaskKind) -> Result<serde_json::value::Value, String> {
    match kind {
        TaskKind::HtmlFetch { url, .. } => fetch_json(st, url, &[]).await,
        TaskKind::Extract { url, selectors } => fetch_json(st, url, selectors).await,
        TaskKind::FormSubmit { url, method, fields, token_field, token } => {
            submit_json(st, url, method, fields, token_field, token).await
        }
        TaskKind::Solve { kind, script, payload_b64, deadline_ms, profile_id } => {
            solve_json(st, kind, script, payload_b64, deadline_ms, profile_id).await
        }
    }
}

fn profile_for(st: &AppState, url: &str) -> (usize, Arc<Profile>) {
    let host = host_of(url);
    let slot = (xxh3::hash(host.as_bytes()) as usize) % st.profiles.len().max(1);
    let profile = reslot(st.profiles[slot].as_ref(), st.asn);
    (slot, profile)
}

async fn fetch_json(st: &AppState, url: &str, selectors: &[(String, String)]) -> Result<serde_json::value::Value, String> {
    let (slot, profile) = profile_for(st, url);
    let mut session = Session::new(profile, url);
    let f = fetch_page_sel(&st.engines, slot, &mut session, url, selectors)
        .await
        .map_err(|e| e.to_string())?;
    st.stats.add_fetch(f.bytes_in);
    let mut v = page_json(&f);
    if let Some(tok) = solve_challenge(st, &mut session, &f).await {
        v["solvedToken"] = serde_json::Value::String(tok.to_string());
    }
    if let Some(route) = f.page.telemetry_route.clone() {
        let flow = telemetry_flow(st, slot, &mut session, &route).await;
        v["telemetry"] = flow;
    }
    if let (Some(url), Some(script)) = (&f.page.challenge_script_url, &f.page.challenge) {
        if st.monitor.check(url.as_str(), script) {
            tracing::warn!(url = url.as_str(), "challenge build changed");
        }
    }
    Ok(v)
}

const TELEMETRY_WINDOW_US: u64 = 25_000_000;
const TELEMETRY_MAX_TICKS: u32 = 4_000;

async fn telemetry_flow(
    st: &AppState,
    slot: usize,
    session: &mut Session,
    route: &parser_pipeline::TelemetryRoute,
) -> serde_json::value::Value {
    let persona = Persona::derive(session.profile.canvas_seed, session.id.as_raw());
    let mut rng = payload_gen::input::SplitMix64Rng::new(session.profile.canvas_seed ^ 0x5E55_0000_0000_0111);
    let vw = session.profile.screen_w.max(320) as f64;
    let vh = session.profile.screen_h.max(240) as f64;
    let from = (vw * (0.08 + rng.next_f64() * 0.1), vh * (0.1 + rng.next_f64() * 0.12));
    let target = (vw * (0.45 + rng.next_f64() * 0.25), vh * (0.4 + rng.next_f64() * 0.25));
    let mut tab = TabSession::start(persona, session.profile.canvas_seed, session.id.as_raw(), from, target, 30.0, None, 0, 0);
    let mut batcher = TelemetryBatcher::new(BATCH_CAP, BATCH_INTERVAL_US);
    let mut scratch: Vec<RawEvent> = Vec::with_capacity(BATCH_CAP + 32);
    let mut now = 0u64;
    let mut ticks = 0u32;
    let mut batches = 0u32;
    let mut events_total = 0u64;
    let mut statuses: Vec<u16> = Vec::new();
    batcher.begin(0);
    while now < TELEMETRY_WINDOW_US && ticks < TELEMETRY_MAX_TICKS {
        let tick = tab.advance(now);
        now = tick.next_due_us.max(now + 1);
        ticks += 1;
        if tick.events.is_empty() {
            if batcher.ready(now) && !scratch.is_empty() {
                batches += 1;
                let status = push_telemetry(&st.engines, slot, session, route, events_bytes(&scratch))
                    .await
                    .map(|s| s)
                    .unwrap_or(0);
                st.stats.add_touches(scratch.len() as u64);
                statuses.push(status);
                scratch.clear();
                batcher.begin(now);
            }
            if tab.finished() {
                break;
            }
            continue;
        }
        batcher.feed(tick.events.len());
        events_total += tick.events.len() as u64;
        scratch.extend_from_slice(&tick.events);
        if batcher.ready(now) {
            batches += 1;
            let status = push_telemetry(&st.engines, slot, session, route, events_bytes(&scratch))
                .await
                .map(|s| s)
                .unwrap_or(0);
            st.stats.add_touches(scratch.len() as u64);
            statuses.push(status);
            scratch.clear();
            batcher.begin(now);
        }
    }
    if !scratch.is_empty() {
        batches += 1;
        let status = push_telemetry(&st.engines, slot, session, route, events_bytes(&scratch))
            .await
            .map(|s| s)
            .unwrap_or(0);
        st.stats.add_touches(scratch.len() as u64);
        statuses.push(status);
    }
    serde_json::json!({
        "provider": route.provider.as_str(),
        "endpoint": route.endpoint.as_str(),
        "transport": format!("{:?}", route.transport),
        "batches": batches,
        "events": events_total,
        "statuses": statuses,
        "lastPhase": format!("{:?}", tab.phase()),
    })
}

fn page_json(f: &Fetched) -> serde_json::value::Value {
    serde_json::json!({
        "finalUrl": f.uri.as_str(),
        "statusCode": f.status,
        "htmlLen": f.bytes_in,
        "title": f.page.title,
        "metaDescription": f.page.meta_description,
        "forms": f.page.forms.iter().map(|fm| serde_json::json!({
            "action": fm.action,
            "method": fm.method,
            "fields": fm.fields.iter().map(|fd| {
                let mut kind = String::with_capacity(8);
                use std::fmt::Write;
                let _ = write!(kind, "{:?}", fd.kind);
                serde_json::json!({ "name": fd.name, "value": fd.value, "kind": kind })
            }).collect::<Vec<_>>(),
        })).collect::<Vec<_>>(),
        "tokens": f.page.tokens.iter().map(|t| t.as_str()).collect::<Vec<_>>(),
        "challengeMarkers": f.page.challenge_markers.iter().map(|(u, _)| u.as_str()).collect::<Vec<_>>(),
        "extracted": f.page.extracted,
        "elapsedMs": f.elapsed_ms,
    })
}

async fn submit_json(
    st: &AppState,
    url: &str,
    method: &Option<String>,
    fields: &[(String, String)],
    token_field: &Option<String>,
    token: &Option<String>,
) -> Result<serde_json::value::Value, String> {
    let (slot, profile) = profile_for(st, url);
    let mut session = Session::new(profile, url);
    let f = fetch_page_sel(&st.engines, slot, &mut session, url, &[])
        .await
        .map_err(|e| e.to_string())?;
    st.stats.add_fetch(f.bytes_in);
    let mut body: Vec<(String, String)> = Vec::with_capacity(8 + fields.len());
    for fm in &f.page.forms {
        for fd in &fm.fields {
            body.push((fd.name.to_string(), fd.value.as_deref().unwrap_or("").to_string()));
        }
    }
    for (k, v) in fields {
        upsert(&mut body, k.clone(), v.clone());
    }
    if let (Some(tf), Some(tv)) = (token_field, token) {
        upsert(&mut body, tf.clone(), tv.clone());
    }
    let m = method
        .clone()
        .unwrap_or_else(|| f.page.forms.first().map(|fm| fm.method.to_string()).unwrap_or_else(|| "POST".into()))
        .to_ascii_uppercase();
    if !matches!(m.as_str(), "POST" | "GET" | "PUT" | "PATCH") {
        return Err("bad method".into());
    }
    let client = st.engines.client_for(slot);
    let form: Vec<(&str, &str)> = body.iter().map(|(k, v)| (k.as_str(), v.as_str())).collect();
    let req = match m.as_str() {
        "POST" => client.post(url),
        "GET" => client.get(url),
        "PUT" => client.put(url),
        _ => client.patch(url),
    };
    let resp = req.form(&form).send().await.map_err(|e| e.to_string())?;
    let status = resp.status().as_u16();
    let body_bytes = resp.bytes().await.map_err(|e| e.to_string())?;
    let mut p = StreamPipeline::new(Default::default());
    let _ = p.push(&body_bytes).map_err(|e| e.to_string())?;
    let page = p.finish().map_err(|e| e.to_string())?;
    let mut v = serde_json::json!({ "statusCode": status, "htmlLen": body_bytes.len() });
    if let Some(tok) = solve_challenge(st, &mut session, &Fetched { status, uri: compact_str::CompactString::from(url), page: Arc::new(page), bytes_in: body_bytes.len() as u64, elapsed_ms: 0 }).await {
        v["solvedToken"] = serde_json::Value::String(tok.to_string());
    }
    Ok(v)
}

fn upsert(body: &mut Vec<(String, String)>, k: String, v: String) {
    for (ek, ev) in body.iter_mut() {
        if ek == &k {
            *ev = v;
            return;
        }
    }
    body.push((k, v));
}

fn decode_b64_script(b64: &str) -> Result<String, String> {
    let trimmed = b64.trim_ascii();
    let bytes = core_utils::base64::STANDARD
        .decode_to_vec(trimmed.as_bytes())
        .map_err(|e| {
            let mut s = String::with_capacity(32);
            use std::fmt::Write;
            let _ = write!(s, "payload_b64: {e}");
            s
        })?;
    String::from_utf8(bytes).map_err(|_| "payload_b64: not utf8".into())
}

async fn solve_json(
    st: &AppState,
    kind: &str,
    script: &Option<String>,
    payload_b64: &Option<String>,
    deadline_ms: &Option<u64>,
    profile_id: &Option<u64>,
) -> Result<serde_json::value::Value, String> {
    let profile = match profile_id {
        Some(pid) => st.profiles.get(*pid as usize).cloned().ok_or("profile not found")?,
        None => st.profiles.first().cloned().ok_or("no profile")?,
    };
    let script = match (script, payload_b64) {
        (Some(s), _) => s.clone(),
        (None, Some(b64)) => decode_b64_script(b64)?,
        (None, None) => {
            let mut e = String::with_capacity(24);
            use std::fmt::Write;
            let _ = write!(e, "bytecode for {kind} unavailable");
            return Err(e);
        }
    };
    let timeout = deadline_ms.map(Duration::from_millis).unwrap_or(st.timeout);
    let req = ExecReq {
        domain: xxh3::hash(script.as_bytes()),
        script: bytes::Bytes::from(script.into_bytes()),
        snap: ProfileSnap::from_parts(&profile, "https://challenge.local/", ""),
        timeout,
        doc: None,
    };
    let outcome = st.pool.exec(req).await;
    match outcome.token {
        Some(t) => {
            let mut path = String::with_capacity(16);
            use std::fmt::Write;
            let _ = write!(path, "{:?}", outcome.path);
            Ok(serde_json::json!({ "solvedToken": t, "path": path }))
        }
        None => Err(outcome.err.map(|e| e.to_string()).unwrap_or_else(|| "solve failed".into())),
    }
}

async fn solve_challenge(st: &AppState, session: &mut Session, f: &Fetched) -> Option<compact_str::CompactString> {
    let script = f.page.challenge.clone()?;
    st.stats.add_script();
    let mut cookie_buf: SmallVec<[u8; 256]> = SmallVec::new();
    session.jar.header_into(&mut cookie_buf);
    let snap = ProfileSnap::from_parts(
        session.profile.as_ref(),
        f.uri.as_str(),
        std::str::from_utf8(&cookie_buf).unwrap_or(""),
    );
    let req = ExecReq {
        domain: xxh3::hash(host_of(f.uri.as_str()).as_bytes()),
        script,
        snap,
        timeout: st.timeout,
        doc: Some(Arc::clone(&f.page)),
    };
    st.pool.exec(req).await.token
}

fn host_of(url: &str) -> compact_str::CompactString {
    let rest = url.strip_prefix("https://").or_else(|| url.strip_prefix("http://")).unwrap_or(url);
    let end = rest.find(['/', '?', '#']).unwrap_or(rest.len());
    compact_str::CompactString::new(&rest[..end])
}

pub fn router(state: Arc<AppState>) -> Router {
    Router::new()
        .route("/health", get(|| async { "my-engine: alive" }))
        .route("/stats", get(stats_handler))
        .route("/createTask", post(create_task))
        .route("/getTaskResult", post(get_result))
        .with_state(state)
}

async fn stats_handler(State(s): State<Arc<AppState>>) -> Json<serde_json::value::Value> {
    Json(s.stats())
}

async fn create_task(
    State(s): State<Arc<AppState>>,
    Json(req): Json<CreateTaskReq>,
) -> Result<Json<CreateTaskResp>, (StatusCode, Json<ErrorResp>)> {
    match s.create_task(req.task).await {
        Ok(id) => Ok(Json(CreateTaskResp { task_id: id })),
        Err(e) => Err((StatusCode::BAD_REQUEST, Json(ErrorResp { error: e }))),
    }
}

async fn get_result(
    State(s): State<Arc<AppState>>,
    Json(req): Json<GetResultReq>,
) -> Json<TaskOutcome> {
    Json(s.task_outcome(req.task_id))
}

#[derive(Deserialize)]
pub struct CreateTaskReq { pub task: TaskKind }
#[derive(Serialize)]
pub struct CreateTaskResp { pub task_id: TaskId }
#[derive(Deserialize)]
pub struct GetResultReq { pub task_id: TaskId }
#[derive(Serialize)]
pub struct ErrorResp { pub error: String }