use crossbeam_channel::Sender;

#[derive(Debug, Clone, Copy)]
pub enum Event {
    CacheHit,
    CacheMiss,
    Compile,
    Timeout,
    Oom,
    WasmRun,
    WasmFail,
    Backpressure,
    ParseFail,
    NoResult,
    ExecFail,
    ExecDone(u64),
}

pub type EventTx = Sender<Event>;
