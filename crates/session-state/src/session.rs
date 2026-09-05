use crate::cookie::CookieJar;
use crate::profile::Profile;
use compact_str::CompactString;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TabId(u64);

impl TabId {
    pub fn new() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(1);
        Self(NEXT.fetch_add(1, Ordering::Relaxed))
    }

    pub const fn from_raw(raw: u64) -> Self {
        Self(raw)
    }

    pub const fn as_raw(self) -> u64 {
        self.0
    }
}

impl Default for TabId {
    fn default() -> Self {
        Self::new()
    }
}

pub struct Session {
    pub id: TabId,
    pub jar: CookieJar,
    pub profile: Arc<Profile>,
    pub origin: CompactString,
    pub last_seen_ms: AtomicU64,
}

impl Session {
    pub fn new(profile: Arc<Profile>, origin: &str) -> Self {
        Self {
            id: TabId::new(),
            jar: CookieJar::new(),
            profile,
            origin: CompactString::new(origin),
            last_seen_ms: AtomicU64::new(unix_ms()),
        }
    }

    pub fn touch(&self) {
        self.last_seen_ms.store(unix_ms(), Ordering::Release);
    }

    pub fn age_ms(&self) -> u64 {
        unix_ms().saturating_sub(self.last_seen_ms.load(Ordering::Acquire))
    }
}

fn unix_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}
