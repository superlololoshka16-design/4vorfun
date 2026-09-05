use aho_corasick::AhoCorasick;
use compact_str::CompactString;
use std::sync::LazyLock;
use thiserror::Error;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TelemetryProvider {
    Arkose,
    DataDome,
    Turnstile,
    HCaptcha,
    ReCaptcha,
    InHouse,
}

impl TelemetryProvider {
    pub fn as_str(self) -> &'static str {
        match self {
            TelemetryProvider::Arkose => "arkose",
            TelemetryProvider::DataDome => "datadome",
            TelemetryProvider::Turnstile => "turnstile",
            TelemetryProvider::HCaptcha => "hcaptcha",
            TelemetryProvider::ReCaptcha => "recaptcha",
            TelemetryProvider::InHouse => "in-house",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Transport {
    CdnPost,
    FormField,
    CustomHeader,
}

#[derive(Debug, Clone)]
pub struct TelemetryRoute {
    pub provider: TelemetryProvider,
    pub endpoint: CompactString,
    pub transport: Transport,
    pub field: CompactString,
}

#[derive(Debug, Error)]
pub enum RouteError {
    #[error("no telemetry route in page")]
    NotFound,
}

struct RoutePattern {
    needle: &'static [u8],
    provider: TelemetryProvider,
    endpoint: &'static str,
    transport: Transport,
    field: &'static str,
}

static PATTERNS: &[RoutePattern] = &[
    RoutePattern {
        needle: b"arkoselabs.com/v2",
        provider: TelemetryProvider::Arkose,
        endpoint: "https://client-api.arkoselabs.com/v2",
        transport: Transport::CdnPost,
        field: "",
    },
    RoutePattern {
        needle: b"funcaptcha",
        provider: TelemetryProvider::Arkose,
        endpoint: "https://client-api.arkoselabs.com/v2",
        transport: Transport::CdnPost,
        field: "",
    },
    RoutePattern {
        needle: b"captcha-delivery.com",
        provider: TelemetryProvider::DataDome,
        endpoint: "https://geo.captcha-delivery.com/telemetry",
        transport: Transport::CdnPost,
        field: "datadome",
    },
    RoutePattern {
        needle: b"datadome",
        provider: TelemetryProvider::DataDome,
        endpoint: "https://geo.captcha-delivery.com/telemetry",
        transport: Transport::FormField,
        field: "datadome",
    },
    RoutePattern {
        needle: b"challenges.cloudflare.com/turnstile",
        provider: TelemetryProvider::Turnstile,
        endpoint: "https://challenges.cloudflare.com/turnstile/v0/telemetry",
        transport: Transport::CdnPost,
        field: "",
    },
    RoutePattern {
        needle: b"cdn-cgi/challenge-platform",
        provider: TelemetryProvider::Turnstile,
        endpoint: "https://challenges.cloudflare.com/cdn-cgi/telemetry",
        transport: Transport::CustomHeader,
        field: "cf-chl-telemetry",
    },
    RoutePattern {
        needle: b"hcaptcha.com",
        provider: TelemetryProvider::HCaptcha,
        endpoint: "https://api.hcaptcha.com/checkcaptcha",
        transport: Transport::CdnPost,
        field: "",
    },
    RoutePattern {
        needle: b"recaptcha/api.js",
        provider: TelemetryProvider::ReCaptcha,
        endpoint: "https://www.google.com/recaptcha/api2/userverify",
        transport: Transport::CdnPost,
        field: "",
    },
];

struct InlineHint {
    needle: &'static [u8],
    path: &'static str,
}

static INLINE_HINTS: &[InlineHint] = &[
    InlineHint { needle: b"\"/telemetry\"", path: "/telemetry" },
    InlineHint { needle: b"'/telemetry'", path: "/telemetry" },
    InlineHint { needle: b"/collect?v=", path: "/collect" },
    InlineHint { needle: b"\"/api/telemetry\"", path: "/api/telemetry" },
    InlineHint { needle: b"'/api/telemetry'", path: "/api/telemetry" },
    InlineHint { needle: b"\"/beacon\"", path: "/beacon" },
    InlineHint { needle: b"sendBeacon(", path: "/beacon" },
];

static AC: LazyLock<AhoCorasick> = LazyLock::new(|| {
    let needles: Vec<&[u8]> = PATTERNS.iter().map(|p| p.needle).collect();
    AhoCorasick::builder()
        .match_kind(aho_corasick::MatchKind::LeftmostFirst)
        .build(needles)
        .expect("telemetry route patterns")
});

static INLINE_AC: LazyLock<AhoCorasick> = LazyLock::new(|| {
    let needles: Vec<&[u8]> = INLINE_HINTS.iter().map(|h| h.needle).collect();
    AhoCorasick::builder()
        .match_kind(aho_corasick::MatchKind::LeftmostFirst)
        .build(needles)
        .expect("inline telemetry hints")
});

pub fn detect_route(scripts: &[&str], inline: &[u8], form_action: Option<&str>) -> Result<TelemetryRoute, RouteError> {
    for src in scripts {
        if let Some(mat) = AC.find(src.as_bytes()) {
            let p = &PATTERNS[mat.pattern().as_usize()];
            return Ok(TelemetryRoute {
                provider: p.provider,
                endpoint: CompactString::const_new(p.endpoint),
                transport: p.transport,
                field: CompactString::const_new(p.field),
            });
        }
    }
    if !inline.is_empty() {
        if let Some(mat) = INLINE_AC.find(inline) {
            let hint = &INLINE_HINTS[mat.pattern().as_usize()];
            return Ok(TelemetryRoute {
                provider: TelemetryProvider::InHouse,
                endpoint: CompactString::const_new(hint.path),
                transport: Transport::CdnPost,
                field: CompactString::const_new("telemetry"),
            });
        }
    }
    let _ = form_action;
    Err(RouteError::NotFound)
}
