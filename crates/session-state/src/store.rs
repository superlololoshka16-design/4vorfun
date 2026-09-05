use crate::session::Session;
use scc::HashMap;

pub struct StateStore {
    inner: HashMap<u64, Box<Session>>,
    ttl_ms: u64,
}

impl StateStore {
    pub fn new(ttl_ms: u64) -> Self {
        Self {
            inner: HashMap::new(),
            ttl_ms,
        }
    }

    pub fn park(&self, session: Box<Session>) {
        let key = session.id.as_raw();
        let _ = self.inner.insert_sync(key, session);
    }

    pub fn take(&self, id: u64) -> Option<Box<Session>> {
        self.inner.remove_sync(&id).map(|(_, s)| s)
    }

    pub fn sweep(&self) -> usize {
        let ttl = self.ttl_ms;
        let mut dead = 0usize;
        self.inner.retain_sync(|_, s| {
            let alive = s.age_ms() < ttl;
            if !alive {
                dead += 1;
            }
            alive
        });
        dead
    }

    pub fn len(&self) -> usize {
        self.inner.len()
    }

    pub fn is_empty(&self) -> bool {
        self.inner.is_empty()
    }
}
