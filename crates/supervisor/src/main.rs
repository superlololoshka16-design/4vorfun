mod api;
mod stats;

use clap::{Parser, Subcommand};
use compact_str::CompactString;
use net_client::{EngineSet, Fetched, engine_catalog, reslot};
use parser_pipeline::PageData;
use core_utils::xxh3;
use runtime_exec::{Bundle, Event, EventTx, ExecReq, ProfileSnap, WorkerPool};
use session_state::{Profile, Session, StateStore};
use smallvec::SmallVec;
use std::io::Write;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::Semaphore;

use stats::{StatBlock, StatsRef, p50p99, render_rss};

#[global_allocator]
static GLOBAL: mimalloc::MiMalloc = mimalloc::MiMalloc;

#[derive(Parser)]
#[command(name = "my-engine", version)]
struct Cli {
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    Run {
        #[arg(long, required = true)]
        url: Vec<String>,
        #[arg(long, default_value_t = 2)]
        workers: usize,
        #[arg(long, default_value_t = 150)]
        timeout_ms: u64,
        #[arg(long, default_value_t = 0)]
        asn: u32,
        #[arg(long, default_value = "assets/polyfill.js")]
        polyfill: PathBuf,
    },
    Bench {
        #[arg(long, required = true)]
        url: String,
        #[arg(long, default_value_t = 32)]
        n: usize,
        #[arg(long, default_value_t = 8)]
        concurrency: usize,
        #[arg(long, default_value = "assets/polyfill.js")]
        polyfill: PathBuf,
    },
    Serve {
        #[arg(long, default_value = "127.0.0.1:8817")]
        listen: String,
        #[arg(long, default_value_t = 4)]
        workers: usize,
        #[arg(long, default_value_t = 150)]
        timeout_ms: u64,
        #[arg(long, default_value = "assets/polyfill.js")]
        polyfill: PathBuf,
    },
}

struct Engine {
    pool: Arc<WorkerPool>,
    engines: Arc<EngineSet>,
    catalog: Vec<Arc<Profile>>,
    store: StateStore,
    stats: StatsRef,
    timeout: Duration,
    asn: u32,
}

fn profile_slot_for(host: &str, catalog_len: usize) -> usize {
    (xxh3::hash(host.as_bytes()) as usize) % catalog_len.max(1)
}

impl Engine {
    async fn run_url(&self, url: &str, out: &mut dyn Write) {
        let host = host_of(url);
        let slot = profile_slot_for(host.as_str(), self.catalog.len());
        let profile = reslot(self.catalog[slot].as_ref(), self.asn);
        let mut session = Box::new(Session::new(profile, url));
        match net_client::fetch_page(&self.engines, slot, &mut session, url).await {
            Ok(f) => {
                self.stats.add_fetch(f.bytes_in);
                render_page(out, &f.page);
                if let Some(script) = f.page.challenge {
                    self.stats.add_script();
                    let mut cookie_buf: SmallVec<[u8; 256]> = SmallVec::new();
                    session.jar.header_into(&mut cookie_buf);
                    let snap = ProfileSnap::from_parts(
                        session.profile.as_ref(),
                        f.uri.as_str(),
                        std::str::from_utf8(&cookie_buf).unwrap_or(""),
                    );
                    let req = ExecReq {
                        domain: xxh3::hash(host.as_bytes()),
                        script,
                        snap,
                        timeout: self.timeout,
                    };
                    let outcome = self.pool.exec(req).await;
                    self.stats.add_touches(outcome.touches);
                    match outcome.token {
                        Some(tok) => {
                            let _ = writeln!(
                                out,
                                "  challenge -> {} (path={:?} cache={} {}ms api_touches={})",
                                tok,
                                outcome.path,
                                outcome.cache_hit,
                                outcome.elapsed.as_millis(),
                                outcome.touches
                            );
                        }
                        None => {
                            let _ = writeln!(
                                out,
                                "  challenge failed: {}",
                                outcome.err.map(|e| e.to_string()).unwrap_or_default()
                            );
                        }
                    }
                }
            }
            Err(e) => {
                let _ = writeln!(out, "fetch failed: {e}");
            }
        }
        self.store.park(session);
    }
}

fn host_of(url: &str) -> CompactString {
    let after = url.split_once("://").map(|(_, rest)| rest).unwrap_or(url);
    let host = after.split('/').next().unwrap_or(after);
    CompactString::new(host.split(':').next().unwrap_or(host))
}

fn render_page(out: &mut dyn Write, page: &PageData) {
    let _ = writeln!(
        out,
        "page: {} ({} bytes, {} forms, {} tokens, {} scripts, inline={}, dom_nodes={} dom_pool={})",
        page.title.as_deref().unwrap_or("-"),
        page.bytes_fed,
        page.forms.len(),
        page.tokens.len(),
        page.script_srcs.len(),
        page.inline_count,
        page.dom.len(),
        page.dom.pool_len()
    );
    if let Some(nd) = &page.next_data {
        let _ = writeln!(out, "  next_data: page={} build={}", nd.page, nd.build_id);
    }
    for t in &page.tokens {
        let _ = writeln!(out, "  token: {t}");
    }
    for src in &page.script_srcs {
        let _ = writeln!(out, "  script src: {src}");
    }
    if page.utf8_bad_chunks > 0 || page.truncated || page.parse_errors > 0 {
        let _ = writeln!(
            out,
            "  [warn] utf8_bad={} truncated={} parse_err={}",
            page.utf8_bad_chunks, page.truncated, page.parse_errors
        );
    }
}

fn spawn_event_drain(rx: crossbeam_channel::Receiver<Event>, stats: StatsRef) {
    std::thread::Builder::new()
        .name("silo-drain".into())
        .spawn(move || {
            for ev in rx.iter() {
                match ev {
                    Event::ExecDone(ms) => {
                        tracing::debug!(target = "exec", "exec done in {ms}ms");
                    }
                    other => {
                        tracing::debug!(target = "exec", "{other:?}");
                    }
                }
                stats.ingest_event(ev);
            }
        })
        .expect("drain thread");
}

fn build_engine(
    workers: usize,
    timeout: Duration,
    asn: u32,
    polyfill: &PathBuf,
) -> Result<Engine, String> {
    let stats = Arc::new(StatBlock::new());
    let bundle = Arc::new(Bundle::open(polyfill).map_err(|e| e.to_string())?);
    let (tx, rx): (EventTx, _) = crossbeam_channel::bounded(8192);
    spawn_event_drain(rx, Arc::clone(&stats));
    let pool = WorkerPool::spawn(workers, bundle, tx, 4096)?;
    let catalog = engine_catalog()?;
    let engines = Arc::new(EngineSet::build(&catalog).map_err(|e| e.to_string())?);
    let profiles: Vec<Arc<Profile>> = catalog.iter().map(|c| c.profile.clone()).collect();
    Ok(Engine {
        pool: Arc::new(pool),
        engines,
        catalog: profiles,
        store: StateStore::new(600_000),
        stats,
        timeout,
        asn,
    })
}

fn main() {
    let cli = Cli::parse();
    tracing_subscriber::fmt()
        .compact()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();
    match cli.cmd {
        Cmd::Run {
            url,
            workers,
            timeout_ms,
            asn,
            polyfill,
        } => match build_engine(workers, Duration::from_millis(timeout_ms), asn, &polyfill) {
            Ok(engine) => {
                let rt = tokio::runtime::Builder::new_multi_thread()
                    .enable_all()
                    .build()
                    .expect("tokio runtime");
                let code = rt.block_on(async move {
                    let stdout = std::io::stdout();
                    let mut out = stdout.lock();
                    for u in &url {
                        let _ = writeln!(&mut out, "== {u}");
                        engine.run_url(u, &mut out).await;
                        let _ = out.flush();
                    }
                    engine.stats.render(&mut out);
                    render_rss(&mut out);
                    0
                });
                std::process::exit(code);
            }
            Err(e) => {
                eprintln!("engine init failed: {e}");
                std::process::exit(1);
            }
        },
        Cmd::Serve {
            listen,
            workers,
            timeout_ms,
            polyfill,
        } => match build_engine(workers, Duration::from_millis(timeout_ms), 0, &polyfill) {
            Ok(engine) => {
                let rt = tokio::runtime::Builder::new_multi_thread()
                    .enable_all()
                    .build()
                    .expect("tokio runtime");
                let code = rt.block_on(async move {
                    let monitor = parser_pipeline::VersionMonitor::new();
                    let state = api::AppState::spawn(
                        workers,
                        engine.engines.clone(),
                        engine.catalog.clone(),
                        engine.pool.clone(),
                        engine.stats.clone(),
                        engine.timeout,
                        engine.asn,
                        monitor,
                    );
                    let app = api::router(state);
                    let listener = tokio::net::TcpListener::bind(&listen).await.expect("bind");
                    tracing::info!("listening: http://{}", listen);
                    axum::serve(listener, app).await.expect("server");
                    0
                });
                std::process::exit(code);
            }
            Err(e) => {
                eprintln!("engine init failed: {e}");
                std::process::exit(1);
            }
        },
        Cmd::Bench {
            url,
            n,
            concurrency,
            polyfill,
        } => match build_engine(2, Duration::from_millis(150), 0, &polyfill) {
            Ok(engine) => {
                let rt = tokio::runtime::Builder::new_multi_thread()
                    .enable_all()
                    .build()
                    .expect("tokio runtime");
                let code = rt.block_on(async move {
                    let host = host_of(&url);
                    let slot = profile_slot_for(host.as_str(), engine.catalog.len());
                    let profile = reslot(engine.catalog[slot].as_ref(), 0);
                    let sem = Arc::new(Semaphore::new(concurrency));
                    let start = Instant::now();
                    let mut handles = Vec::with_capacity(n);
                    for _ in 0..n {
                        let permit = Arc::clone(&sem).acquire_owned().await;
                        let engines = Arc::clone(&engine.engines);
                        let stats = Arc::clone(&engine.stats);
                        let profile = Arc::clone(&profile);
                        let url: Arc<str> = Arc::from(url.as_str());
                        handles.push(tokio::spawn(async move {
                            let t0 = Instant::now();
                            let mut session = Box::new(Session::new(profile, &url));
                            let r: Result<Fetched, _> =
                                net_client::fetch_page(&engines, slot, &mut session, &url).await;
                            drop(permit);
                            match r {
                                Ok(f) => {
                                    stats.add_fetch(f.bytes_in);
                                    Some(t0.elapsed().as_millis() as u64)
                                }
                                Err(_) => None,
                            }
                        }));
                    }
                    let mut durs = Vec::with_capacity(n);
                    for h in handles {
                        if let Ok(Some(ms)) = h.await {
                            durs.push(ms);
                        }
                    }
                    let total = start.elapsed().as_secs_f64();
                    let (p50, p99) = p50p99(durs);
                    let mut stdout = std::io::stdout();
                    let _ = writeln!(
                        &mut stdout,
                        "n={n} p50={p50}ms p99={p99}ms rps={:.2}",
                        n as f64 / total.max(f64::EPSILON)
                    );
                    engine.stats.render(&mut stdout);
                    render_rss(&mut stdout);
                    0
                });
                std::process::exit(code);
            }
            Err(e) => {
                eprintln!("engine init failed: {e}");
                std::process::exit(1);
            }
        },
    }
}
