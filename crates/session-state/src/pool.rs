use arc_swap::ArcSwap;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use crate::profile::Profile;

pub struct ProfilePool {
    inner: ArcSwap<Vec<Arc<Profile>>>,
    revisions: AtomicU64,
}

impl Default for ProfilePool {
    fn default() -> Self {
        Self {
            inner: ArcSwap::from_pointee(Vec::new()),
            revisions: AtomicU64::new(0),
        }
    }
}

impl ProfilePool {
    pub fn new(profiles: Vec<Arc<Profile>>) -> Self {
        Self {
            inner: ArcSwap::from_pointee(profiles),
            revisions: AtomicU64::new(1),
        }
    }

    pub fn snapshot(&self) -> Arc<Vec<Arc<Profile>>> {
        self.inner.load_full()
    }

    pub fn swap(&self, profiles: Vec<Arc<Profile>>) {
        self.inner.store(Arc::new(profiles));
        self.revisions.fetch_add(1, Ordering::Release);
    }

    pub fn revisions(&self) -> u64 {
        self.revisions.load(Ordering::Acquire)
    }
}
