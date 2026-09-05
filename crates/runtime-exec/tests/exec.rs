use bytes::Bytes;
use payload_gen::sha256_hex_into;
use runtime_exec::{Bundle, ExecError, ExecPath, ExecReq, ProfileSnap, WorkerPool};
use session_state::{Family, NetKind, Platform, Profile};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

const V1: &str = r#"var _aXq7 = 11, _zKp2 = 22;
var t = __silo_sha256("silo:" + _aXq7 + ":" + _zKp2);
t;"#;

const V2: &str = r#"var qq1w = 33, mn4r = 44;
var t = __silo_sha256("silo:" + qq1w + ":" + mn4r);
t;"#;

const LOOP: &str = "while (true) { }";

const OOM: &str = r#"var a = [];
for (var i = 0; i < 400000; i++) { a.push({ id: i, tag: "payload-payload-payload" }); }
a.length;"#;

const WASM_MOD: &[u8] = &[
    0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00, 0x01, 0x05, 0x01, 0x60, 0x00, 0x01, 0x7e, 0x03,
    0x02, 0x01, 0x00, 0x07, 0x0a, 0x01, 0x06, 0x61, 0x6e, 0x73, 0x77, 0x65, 0x72, 0x00, 0x00, 0x0a,
    0x06, 0x01, 0x04, 0x00, 0x42, 0x2a, 0x0b,
];

fn test_profile() -> Profile {
    Profile {
        ua: Arc::from(
            "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/149.0.0.0 Safari/537.36",
        ),
        sec_ch_ua: Arc::from(r#""Google Chrome";v="149", "Chromium";v="149""#),
        accept_language: Arc::from("en-US,en;q=0.9"),
        platform: Platform::Windows,
        locale: "en-US".into(),
        tz: "America/New_York".into(),
        screen_w: 1920,
        screen_h: 1080,
        canvas_seed: 0x5EED,
        webgl_vendor: "Google Inc. (NVIDIA)".into(),
        webgl_renderer: "ANGLE (NVIDIA)".into(),
        asn: 7922,
        net: NetKind::Residential,
        family: Family::Chrome { major: 149 },
    }
}

fn snap() -> ProfileSnap {
    ProfileSnap::from_parts(&test_profile(), "https://mock.local/page", "sid=1")
}

fn polyfill_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../supervisor/assets/polyfill.js")
}

fn pool() -> WorkerPool {
    let bundle = Arc::new(Bundle::open(polyfill_path()).expect("polyfill present"));
    let (tx, _rx) = crossbeam_channel::bounded(8192);
    WorkerPool::spawn(1, bundle, tx, 128).expect("pool spawns")
}

fn req(script: &str, timeout_ms: u64) -> ExecReq {
    ExecReq {
        domain: 0xABCD,
        script: Bytes::copy_from_slice(script.as_bytes()),
        snap: snap(),
        timeout: Duration::from_millis(timeout_ms),
    }
}

#[tokio::test]
async fn native_crypto_and_polymorphic_cache_hit() {
    let pool = pool();
    let r = tokio::runtime::Handle::current();
    let _ = r;
    let o1 = pool.exec(req(V1, 1000)).await;
    let tok1 = o1.token.clone().expect("v1 token");
    assert_eq!(o1.path, ExecPath::Compile);
    assert!(!o1.cache_hit);
    let mut expect = [0u8; 64];
    sha256_hex_into(b"silo:11:22", &mut expect);
    assert_eq!(tok1.as_str(), std::str::from_utf8(&expect).unwrap());

    let o2 = pool.exec(req(V2, 1000)).await;
    let tok2 = o2.token.expect("v2 token");
    let mut expect2 = [0u8; 64];
    sha256_hex_into(b"silo:33:44", &mut expect2);
    assert_eq!(tok2.as_str(), std::str::from_utf8(&expect2).unwrap());
    assert_eq!(o2.path, ExecPath::NormHit, "variant must reuse skeleton");
    assert!(o2.cache_hit);

    let o3 = pool.exec(req(V1, 1000)).await;
    assert_eq!(o3.path, ExecPath::RawHit, "exact replay hits raw cache");
    assert_eq!(o3.token.expect("v3 token").as_str(), tok1.as_str());
}

#[tokio::test]
async fn constant_folding_merges_computed_seeds() {
    let a = "var q = 100 + 15; __silo_md5(\"fold:\" + q);";
    let b = "var z = 90 + 25; __silo_md5(\"fold:\" + z);";
    let pool = pool();
    let o1 = pool.exec(req(a, 1000)).await;
    assert!(o1.token.is_some());
    let o2 = pool.exec(req(b, 1000)).await;
    assert_eq!(o2.path, ExecPath::NormHit, "folded literals converge");
    assert_eq!(o1.token, o2.token);
}

#[tokio::test]
async fn infinite_loop_hits_deadline_and_worker_survives() {
    let pool = pool();
    let o = pool.exec(req(LOOP, 120)).await;
    assert!(matches!(o.err, Some(ExecError::Timeout)));
    assert!(o.token.is_none());
    let after = pool.exec(req(V1, 1000)).await;
    assert!(after.token.is_some(), "worker must survive interrupt");
}

#[tokio::test]
async fn oom_rebuilds_engine_and_recovers() {
    let pool = pool();
    let o = pool.exec(req(OOM, 4000)).await;
    assert!(
        matches!(o.err, Some(ExecError::Oom)) || matches!(o.err, Some(ExecError::Timeout)),
        "oom path: {:?}",
        o.err
    );
    let after = pool.exec(req(V1, 1000)).await;
    assert!(after.token.is_some(), "engine must recover after oom");
}

#[tokio::test]
async fn garbage_script_is_graceful_parse_error() {
    let pool = pool();
    let junk = [
        0x01u8, 0x02, b' ', b'n', b'o', b't', b'-', b'j', b's', 0xFF, 0xFE,
    ];
    let mut r = req("x", 500);
    r.script = Bytes::copy_from_slice(&junk);
    let o = pool.exec(r).await;
    assert!(matches!(o.err, Some(ExecError::Parse)) || matches!(o.err, Some(ExecError::Utf8)));
    let o2 = pool.exec(req(V2, 1000)).await;
    assert!(o2.token.is_some());
}

#[tokio::test]
async fn wasm_module_runs_with_fuel() {
    let pool = pool();
    let mut r = req("x", 1000);
    r.script = Bytes::copy_from_slice(WASM_MOD);
    let o = pool.exec(r).await;
    assert_eq!(o.path, ExecPath::Wasm);
    assert_eq!(o.token.expect("wasm answer").as_str(), "42");
}

#[tokio::test]
async fn math_random_seeded_per_profile() {
    let pool = pool();
    let o = pool
        .exec(req(
            "var a = Math.random(); var b = Math.random(); a + b;",
            1000,
        ))
        .await;
    assert!(o.token.is_some());
    let again = pool
        .exec(req(
            "var a = Math.random(); var b = Math.random(); a + b;",
            1000,
        ))
        .await;
    assert_eq!(o.token, again.token, "same seed reproduces sequence");
}

#[test]
fn zero_workers_rejected() {
    let bundle = Arc::new(Bundle::open(polyfill_path()).expect("polyfill"));
    let (tx, _rx) = crossbeam_channel::bounded(8);
    assert!(WorkerPool::spawn(0, bundle, tx, 16).is_err());
}

#[tokio::test]
async fn touch_instrumentation_reports_api_coverage() {
    let pool = pool();
    let o = pool
        .exec(req(
            "var u = navigator.userAgent; var w = screen.width; u.length + w;",
            1000,
        ))
        .await;
    assert!(o.token.is_some());
    assert!(
        o.touches >= 2,
        "navigator+screen reads must be instrumented, got {}",
        o.touches
    );
}

#[tokio::test]
async fn no_silo_names_leak_into_window() {
    let pool = pool();
    let probe = "var leak = 0;\
        var names = Object.getOwnPropertyNames(globalThis);\
        for (var i = 0; i < names.length; i++) { if (names[i].indexOf(\"__silo\") === 0) { leak++; } }\
        var probe2 = (\"__silo_sha256\" in globalThis) || (\"__silo_profile\" in globalThis) || (\"__silo_cache\" in globalThis);\
        leak * 2 + (probe2 ? 1 : 0);";
    let o = pool.exec(req(probe, 1000)).await;
    assert_eq!(
        o.token.as_deref(),
        Some("0"),
        "window must not expose a single __silo_* name, got {:?}",
        o.token
    );
}

#[tokio::test]
async fn polyfill_profile_reaches_canvas_stub() {
    let pool = pool();
    let probe = "var c = document.createElement(\"canvas\");\
        var gl = c.getContext(\"webgl\");\
        var v = gl.getParameter(gl.getExtension(\"WEBGL_debug_renderer_info\").UNMASKED_VENDOR_WEBGL);\
        v.indexOf(\"NVIDIA\") >= 0;";
    let o = pool.exec(req(probe, 1000)).await;
    assert_eq!(o.token.as_deref(), Some("true"), "webgl vendor из профиля");
}
