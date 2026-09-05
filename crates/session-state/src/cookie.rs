use compact_str::CompactString;
use indexmap::IndexMap;
use smallvec::SmallVec;

#[derive(Default)]
pub struct CookieJar {
    map: IndexMap<CompactString, CompactString, ahash::RandomState>,
}

impl CookieJar {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn len(&self) -> usize {
        self.map.len()
    }

    pub fn is_empty(&self) -> bool {
        self.map.is_empty()
    }

    pub fn ingest(&mut self, set_cookie: &str) {
        let line = set_cookie.split(';').next().unwrap_or("").trim();
        let Some(eq) = line.find('=') else {
            return;
        };
        let (name, value) = line.split_at(eq);
        let value = &value[1..];
        if name.is_empty() {
            return;
        }
        self.map
            .insert(CompactString::new(name), CompactString::new(value));
    }

    pub fn get(&self, name: &str) -> Option<&str> {
        self.map.get(name).map(|v| v.as_str())
    }

    pub fn header_into(&self, out: &mut SmallVec<[u8; 256]>) {
        for (i, (k, v)) in self.map.iter().enumerate() {
            if i > 0 {
                out.push(b';');
                out.push(b' ');
            }
            out.extend_from_slice(k.as_bytes());
            out.push(b'=');
            out.extend_from_slice(v.as_bytes());
        }
    }
}
