use std::io::Write;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use runtime_exec::Event;

#[repr(align(64))]
pub struct StatBlock {
    pub started_at: std::time::Instant,
    fetches: AtomicU64,
    bytes_in: AtomicU64,
    scripts: AtomicU64,
    cache_hits: AtomicU64,
    cache_misses: AtomicU64,
    compiles: AtomicU64,
    timeouts: AtomicU64,
    errors: AtomicU64,
    backpressure: AtomicU64,
    wasm_runs: AtomicU64,
    api_touches: AtomicU64,
}

impl Default for StatBlock {
    fn default() -> Self {
        Self::new()
    }
}

impl StatBlock {
    pub fn new() -> Self {
        Self {
            started_at: std::time::Instant::now(),
            fetches: AtomicU64::new(0),
            bytes_in: AtomicU64::new(0),
            scripts: AtomicU64::new(0),
            cache_hits: AtomicU64::new(0),
            cache_misses: AtomicU64::new(0),
            compiles: AtomicU64::new(0),
            timeouts: AtomicU64::new(0),
            errors: AtomicU64::new(0),
            backpressure: AtomicU64::new(0),
            wasm_runs: AtomicU64::new(0),
            api_touches: AtomicU64::new(0),
        }
    }

    pub fn ingest_event(&self, ev: Event) {
        match ev {
            Event::CacheHit => {
                self.cache_hits.fetch_add(1, Ordering::Relaxed);
            }
            Event::ExecDone(_) => {}
            Event::CacheMiss => {
                self.cache_misses.fetch_add(1, Ordering::Relaxed);
            }
            Event::Compile => {
                self.compiles.fetch_add(1, Ordering::Relaxed);
            }
            Event::Timeout => {
                self.timeouts.fetch_add(1, Ordering::Relaxed);
            }
            Event::Oom | Event::ExecFail | Event::ParseFail | Event::NoResult | Event::WasmFail => {
                self.errors.fetch_add(1, Ordering::Relaxed);
            }
            Event::Backpressure => {
                self.backpressure.fetch_add(1, Ordering::Relaxed);
            }
            Event::WasmRun => {
                self.wasm_runs.fetch_add(1, Ordering::Relaxed);
            }
        }
    }

    pub fn add_fetch(&self, bytes: u64) {
        self.fetches.fetch_add(1, Ordering::Relaxed);
        self.bytes_in.fetch_add(bytes, Ordering::Relaxed);
    }

    pub fn fetches(&self) -> u64 { self.fetches.load(Ordering::Relaxed) }
    pub fn bytes(&self) -> u64 { self.bytes_in.load(Ordering::Relaxed) }
    pub fn scripts(&self) -> u64 { self.scripts.load(Ordering::Relaxed) }

    pub fn add_script(&self) {
        self.scripts.fetch_add(1, Ordering::Relaxed);
    }

    pub fn add_touches(&self, n: u64) {
        self.api_touches.fetch_add(n, Ordering::Relaxed);
    }

    pub fn touches(&self) -> u64 {
        self.api_touches.load(Ordering::Relaxed)
    }

    pub fn uptime_secs(&self) -> u64 {
        self.started_at.elapsed().as_secs()
    }

    pub fn render(&self, out: &mut dyn Write) {
        let fetches = self.fetches.load(Ordering::Relaxed);
        let bytes_in = self.bytes_in.load(Ordering::Relaxed);
        let scripts = self.scripts.load(Ordering::Relaxed);
        let hits = self.cache_hits.load(Ordering::Relaxed);
        let misses = self.cache_misses.load(Ordering::Relaxed);
        let compiles = self.compiles.load(Ordering::Relaxed);
        let timeouts = self.timeouts.load(Ordering::Relaxed);
        let errors = self.errors.load(Ordering::Relaxed);
        let backpressure = self.backpressure.load(Ordering::Relaxed);
        let wasm = self.wasm_runs.load(Ordering::Relaxed);
        let api_touches = self.api_touches.load(Ordering::Relaxed);
        let uptime = self.uptime_secs();
        let _ = writeln!(
            out,
            "fetches={fetches} bytes_in={bytes_in} scripts={scripts} cache_hits={hits} cache_misses={misses} compiles={compiles} timeouts={timeouts} errors={errors} backpressure={backpressure} wasm={wasm} api_touches={api_touches} uptime={uptime}s"
        );
    }
}

pub fn render_rss(out: &mut dyn Write) {
    let mut buf = String::new();
    if std::fs::File::open("/proc/self/status")
        .and_then(|mut f| std::io::Read::read_to_string(&mut f, &mut buf))
        .is_err()
    {
        let _ = writeln!(out, "rss=unavailable");
        return;
    }
    let mut rss = None;
    let mut hwm = None;
    for line in buf.lines() {
        if let Some(rest) = line.strip_prefix("VmRSS:") {
            rss = Some(compact_str::CompactString::new(rest.trim()));
        } else if let Some(rest) = line.strip_prefix("VmHWM:") {
            hwm = Some(compact_str::CompactString::new(rest.trim()));
        }
    }
    let _ = writeln!(
        out,
        "rss={} hwm={}",
        rss.unwrap_or_else(|| "n/a".into()),
        hwm.unwrap_or_else(|| "n/a".into())
    );
}

pub fn p50p99(mut durs: Vec<u64>) -> (u64, u64) {
    if durs.is_empty() {
        return (0, 0);
    }
    durs.sort_unstable();
    let p50 = durs[(durs.len() - 1) / 2];
    let p99 = durs[((durs.len() - 1) * 99) / 100];
    (p50, p99)
}

pub type StatsRef = Arc<StatBlock>;
