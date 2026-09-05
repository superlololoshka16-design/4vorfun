use aho_corasick::AhoCorasick;
use std::sync::OnceLock;

pub struct ProbeVerdict {
    pub distinct: u32,
    pub looks_like_lib: bool,
}

struct Probe {
    markers: AhoCorasick,
    libs: AhoCorasick,
}

const MARKERS: &[&str] = &[
    "eval(",
    "atob(",
    "btoa(",
    "Function(",
    "String.fromCharCode",
    "setTimeout(",
    "setInterval(",
    "document.write",
    "execScript",
    "charCodeAt",
];

const LIBS: &[&str] = &[
    "jQuery",
    "jquery",
    "React.createElement",
    "react-dom",
    "vue.runtime",
    "Vue.directive",
    "__NEXT_DATA__",
    "webpackChunk",
    "angular.module",
    "svelte",
];

static PROBE: OnceLock<Probe> = OnceLock::new();

fn probe() -> &'static Probe {
    PROBE.get_or_init(|| Probe {
        markers: AhoCorasick::new(MARKERS).expect("static markers"),
        libs: AhoCorasick::new(LIBS).expect("static lib signatures"),
    })
}

impl Probe {
    fn scan(&self, script: &[u8]) -> ProbeVerdict {
        let mut seen = [false; MARKERS.len()];
        for mat in self.markers.find_iter(script) {
            let id = mat.pattern().as_usize();
            if id < seen.len() {
                seen[id] = true;
            }
        }
        let distinct: u32 = seen.iter().map(|&b| u32::from(b)).sum();
        ProbeVerdict {
            distinct,
            looks_like_lib: self.libs.is_match(script),
        }
    }
}

pub fn is_challenge(script: &[u8]) -> bool {
    if script.len() < 384 {
        return false;
    }
    let v = probe().scan(script);
    v.distinct >= 3 && !v.looks_like_lib
}
