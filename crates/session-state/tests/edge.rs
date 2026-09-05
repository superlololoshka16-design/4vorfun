use compact_str::CompactString;
use session_state::{
    CookieJar, Family, NetKind, Platform, Profile, ProfilePool, Session, StateStore,
};
use smallvec::SmallVec;
use std::sync::Arc;

fn chrome_profile(platform: Platform, ua: &str, sec: &str) -> Profile {
    Profile {
        ua: Arc::from(ua),
        sec_ch_ua: Arc::from(sec),
        accept_language: Arc::from("en-US,en;q=0.9"),
        platform,
        locale: "en-US".into(),
        tz: "America/New_York".into(),
        screen_w: 1920,
        screen_h: 1080,
        canvas_seed: 42,
        webgl_vendor: "Google Inc. (NVIDIA)".into(),
        webgl_renderer: "ANGLE (NVIDIA)".into(),
        asn: 7922,
        net: NetKind::Residential,
        family: Family::Chrome { major: 149 },
    }
}

const UA_WIN: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/149.0.0.0 Safari/537.36";
const SEC_149: &str = r#""Google Chrome";v="149", "Chromium";v="149", "Not)A;Brand";v="24""#;

#[test]
fn profile_rejects_platform_ua_mismatch() {
    let mut p = chrome_profile(Platform::MacOS, UA_WIN, SEC_149);
    let err = p.validate().unwrap_err();
    assert!(err.to_string().contains("platform-token"));
    p.platform = Platform::Windows;
    assert!(p.validate().is_ok());
}

#[test]
fn profile_rejects_sec_ch_ua_major_mismatch() {
    let mut p = chrome_profile(
        Platform::Windows,
        UA_WIN,
        r#""Google Chrome";v="148", "Chromium";v="148""#,
    );
    let err = p.validate().unwrap_err();
    assert!(err.to_string().contains("sec-ch-ua-major"));
    p.sec_ch_ua = Arc::from(SEC_149);
    assert!(p.validate().is_ok());
}

#[test]
fn profile_rejects_headless_marks() {
    let mut p = chrome_profile(Platform::Windows, UA_WIN, SEC_149);
    p.ua = Arc::from("Mozilla/5.0 (Windows NT 10.0) HeadlessChrome/149.0.0.0");
    assert!(p.validate().is_err());
}

#[test]
fn profile_rejects_zero_screen_and_bad_locale() {
    let mut p = chrome_profile(Platform::Windows, UA_WIN, SEC_149);
    p.screen_w = 0;
    assert!(p.validate().is_err());
    p.screen_w = 1920;
    p.locale = CompactString::new("enus");
    assert!(p.validate().is_err());
}

#[test]
fn cookie_jar_dedups_and_keeps_order() {
    let mut jar = CookieJar::new();
    jar.ingest("sid=aaa; Path=/; HttpOnly");
    jar.ingest("csrf=bbb");
    jar.ingest("sid=zzz; Max-Age=3600");
    jar.ingest("");
    jar.ingest("novalue");
    jar.ingest("broken");
    assert_eq!(jar.len(), 2);
    let mut out: SmallVec<[u8; 256]> = SmallVec::new();
    jar.header_into(&mut out);
    assert_eq!(out.as_slice(), b"sid=zzz; csrf=bbb");
    assert_eq!(jar.get("sid"), Some("zzz"));
}

#[test]
fn store_park_take_and_ttl_sweep() {
    let store = StateStore::new(0);
    let p = Arc::new(chrome_profile(Platform::Windows, UA_WIN, SEC_149));
    let s = Box::new(Session::new(p, "https://a.example"));
    let id = s.id;
    store.park(s);
    assert!(store.take(id.as_raw()).is_some());
    assert!(store.take(id.as_raw()).is_none());
    let p2 = Arc::new(chrome_profile(Platform::Windows, UA_WIN, SEC_149));
    let s2 = Box::new(Session::new(p2, "https://b.example"));
    let id2 = s2.id;
    store.park(s2);
    std::thread::sleep(std::time::Duration::from_millis(5));
    assert_eq!(store.sweep(), 1);
    assert!(store.take(id2.as_raw()).is_none());
}

#[test]
fn profile_pool_snapshot_and_swap() {
    let p = Arc::new(chrome_profile(Platform::Windows, UA_WIN, SEC_149));
    let pool = ProfilePool::new(vec![p]);
    assert_eq!(pool.snapshot().len(), 1);
    pool.swap(Vec::new());
    assert_eq!(pool.snapshot().len(), 0);
    assert_eq!(pool.revisions(), 2);
}
