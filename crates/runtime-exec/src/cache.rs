use crate::normalize::Lit;
use core_utils::xxh3;
use scc::HashMap;
use smallvec::SmallVec;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

pub(crate) struct RawEntry {
    pub skel: u64,
    pub args: Arc<SmallVec<[Lit; 16]>>,
    used: AtomicU64,
}

pub struct NormCache {
    raw: HashMap<(u64, u64), Arc<RawEntry>>,
    src: HashMap<(u64, u64), Arc<str>>,
    cap: usize,
}

fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

impl NormCache {
    pub fn new(cap: usize) -> Self {
        Self {
            raw: HashMap::new(),
            src: HashMap::new(),
            cap,
        }
    }

    pub fn raw_hash(script: &[u8]) -> u64 {
        xxh3::hash(script)
    }

    pub(crate) fn lookup_raw(
        &self,
        domain: u64,
        raw: u64,
    ) -> Option<(u64, Arc<SmallVec<[Lit; 16]>>)> {
        self.raw.read_sync(&(domain, raw), |_, v| {
            v.used.store(now_secs(), Ordering::Relaxed);
            (v.skel, Arc::clone(&v.args))
        })
    }

    pub(crate) fn put_raw(&self, domain: u64, raw: u64, skel: u64, args: SmallVec<[Lit; 16]>) {
        let _ = self.raw.insert_sync(
            (domain, raw),
            Arc::new(RawEntry {
                skel,
                args: Arc::new(args),
                used: AtomicU64::new(now_secs()),
            }),
        );
    }

    pub(crate) fn lookup_src(&self, domain: u64, skel: u64) -> Option<Arc<str>> {
        self.src.read_sync(&(domain, skel), |_, v| Arc::clone(v))
    }

    pub(crate) fn put_src(&self, domain: u64, skel: u64, src: Arc<str>) {
        let _ = self.src.insert_sync((domain, skel), src);
    }

    pub fn sweep(&self, max_age_secs: u64) {
        let now = now_secs();
        self.raw
            .retain_sync(|_, v| now.saturating_sub(v.used.load(Ordering::Relaxed)) < max_age_secs);
        if self.raw.len() > self.cap {
            self.raw.clear_sync();
        }
        if self.src.len() > self.cap {
            self.src.clear_sync();
        }
    }

    pub fn raw_len(&self) -> usize {
        self.raw.len()
    }

    pub fn src_len(&self) -> usize {
        self.src.len()
    }
}
