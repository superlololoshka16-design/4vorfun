use bytes::Bytes;
use compact_str::CompactString;
use payload_gen::canvas_hash;
use session_state::Profile;
use std::sync::Arc;
use std::time::Duration;

pub struct ProfileSnap {
    pub ua: Arc<str>,
    pub platform: CompactString,
    pub locale: CompactString,
    pub tz: CompactString,
    pub screen_w: u32,
    pub screen_h: u32,
    pub mobile: bool,
    pub webgl_vendor: CompactString,
    pub webgl_renderer: CompactString,
    pub canvas_hash_hex: CompactString,
    pub href: CompactString,
    pub cookie: CompactString,
    pub seed: u64,
}

impl ProfileSnap {
    pub fn from_parts(profile: &Profile, href: &str, cookie: &str) -> Self {
        let ch = canvas_hash(
            profile.canvas_seed,
            profile.webgl_vendor.as_str(),
            profile.webgl_renderer.as_str(),
        );
        Self {
            ua: Arc::clone(&profile.ua),
            platform: profile.platform.as_str().into(),
            locale: profile.locale.clone(),
            tz: profile.tz.clone(),
            screen_w: profile.screen_w,
            screen_h: profile.screen_h,
            mobile: profile.platform.is_mobile(),
            webgl_vendor: profile.webgl_vendor.clone(),
            webgl_renderer: profile.webgl_renderer.clone(),
            canvas_hash_hex: parse_hex_token(&ch),
            href: CompactString::new(href),
            cookie: CompactString::new(cookie),
            seed: profile.canvas_seed,
        }
    }
}

pub struct ExecReq {
    pub domain: u64,
    pub script: Bytes,
    pub snap: ProfileSnap,
    pub timeout: Duration,
}

pub(crate) struct ExecTask {
    pub req: ExecReq,
    pub reply: tokio::sync::oneshot::Sender<ExecOutcome>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExecPath {
    RawHit,
    NormHit,
    Compile,
    Wasm,
}

#[derive(Debug)]
pub enum ExecError {
    Utf8,
    Parse,
    NoResult,
    Timeout,
    Oom,
    Backpressure,
    Shutdown,
    WasmCompile,
    WasmImports,
    WasmFuel,
    Panic,
    Js(String),
}

impl std::fmt::Display for ExecError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let label = match self {
            ExecError::Utf8 => "script not utf8",
            ExecError::Parse => "script parse failed",
            ExecError::NoResult => "no result value",
            ExecError::Timeout => "deadline exceeded",
            ExecError::Oom => "engine oom",
            ExecError::Backpressure => "worker queue full",
            ExecError::Shutdown => "pool down",
            ExecError::WasmCompile => "wasm compile failed",
            ExecError::WasmImports => "wasm imports unsupported",
            ExecError::WasmFuel => "wasm fuel exhausted",
            ExecError::Panic => "worker panic",
            ExecError::Js(m) => return write!(f, "js exception: {m}"),
        };
        f.write_str(label)
    }
}

/// hex всегда ASCII — [u8; 32] -> CompactString, ноль аллокаций на куче.
#[inline(always)]
fn parse_hex_token(ch: &[u8; 32]) -> CompactString {
    let mut s = compact_str::CompactString::with_capacity(64);
    for b in ch {
        s.push(b"0123456789abcdef"[(*b >> 4) as usize] as char);
        s.push(b"0123456789abcdef"[(*b & 0xf) as usize] as char);
    }
    s
}

impl std::error::Error for ExecError {}

pub struct ExecOutcome {
    pub token: Option<CompactString>,
    pub path: ExecPath,
    pub cache_hit: bool,
    pub elapsed: Duration,
    pub err: Option<ExecError>,
    pub touches: u64,
}
