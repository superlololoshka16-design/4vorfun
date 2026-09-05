use crate::cache::NormCache;
use crate::events::EventTx;
use crate::polyfill::Bundle;
use crate::task::{ExecError, ExecOutcome, ExecPath, ExecReq, ExecTask};
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::time::Duration;

pub struct WorkerPool {
    tx: crossbeam_channel::Sender<ExecTask>,
    joins: Vec<std::thread::JoinHandle<()>>,
    cache: Arc<NormCache>,
    events: EventTx,
}

const QUEUE_CAP: usize = 1024;
const REPLY_GUARD_MS: u64 = 250;

fn outcome(err: ExecError) -> ExecOutcome {
    ExecOutcome {
        token: None,
        path: ExecPath::Compile,
        cache_hit: false,
        elapsed: Duration::ZERO,
        err: Some(err),
        touches: 0,
    }
}

impl WorkerPool {
    pub fn spawn(
        workers: usize,
        bundle: Arc<Bundle>,
        events: EventTx,
        cache_cap: usize,
    ) -> Result<Self, String> {
        if workers == 0 {
            return Err("at least one worker required".into());
        }
        let cache = Arc::new(NormCache::new(cache_cap));
        let (tx, rx) = crossbeam_channel::bounded::<ExecTask>(QUEUE_CAP);
        let mut joins = Vec::with_capacity(workers);
        for i in 0..workers {
            let rx = rx.clone();
            let bundle = Arc::clone(&bundle);
            let events = events.clone();
            let cache = Arc::clone(&cache);
            let join = std::thread::Builder::new()
                .name({
                    use core::fmt::Write;
                    let mut s = compact_str::CompactString::new("");
                    let _ = write!(s, "silo-js-{i}");
                    s.into()
                })
                .spawn(move || crate::worker::worker_main(i, rx, bundle, events, cache))
                .map_err(|e| e.to_string())?;
            joins.push(join);
        }
        Ok(Self {
            tx,
            joins,
            cache,
            events,
        })
    }

    pub fn cache(&self) -> &Arc<NormCache> {
        &self.cache
    }

    pub fn live_workers(&self) -> usize {
        self.joins.iter().filter(|j| !j.is_finished()).count()
    }

    pub fn exec(&self, req: ExecReq) -> Pin<Box<dyn Future<Output = ExecOutcome> + Send>> {
        let (r_tx, r_rx) = tokio::sync::oneshot::channel();
        let guard = req.timeout + Duration::from_millis(REPLY_GUARD_MS);
        match self.tx.try_send(ExecTask { req, reply: r_tx }) {
            Ok(()) => Box::pin(async move {
                match tokio::time::timeout(guard, r_rx).await {
                    Ok(Ok(outcome)) => outcome,
                    Ok(Err(_)) => outcome(ExecError::Shutdown),
                    Err(_) => outcome(ExecError::Timeout),
                }
            }),
            Err(crossbeam_channel::TrySendError::Full(_)) => {
                let _ = self.events.try_send(crate::events::Event::Backpressure);
                Box::pin(std::future::ready(outcome(ExecError::Backpressure)))
            }
            Err(crossbeam_channel::TrySendError::Disconnected(_)) => {
                Box::pin(std::future::ready(outcome(ExecError::Shutdown)))
            }
        }
    }

    pub fn join(self) {
        drop(self.tx);
        for j in self.joins {
            let _ = j.join();
        }
    }
}
