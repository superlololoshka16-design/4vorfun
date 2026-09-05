use net_client::{EngineSet, engine_catalog, fetch_page, reslot};
use session_state::{Profile, Session};
use std::time::Duration;

#[tokio::test]
async fn malformed_url_fails_gracefully() {
    let catalog = engine_catalog().expect("catalog");
    let engines = EngineSet::build(&catalog).expect("engines");
    let profile = reslot(catalog[0].profile.as_ref(), 0);
    let mut session = Session::new(profile, "about:blank");
    let r = fetch_page(&engines, 0, &mut session, "htp:/::bad-url").await;
    assert!(r.is_err());
}

#[tokio::test]
async fn connection_refused_maps_to_transport_error() {
    let catalog = engine_catalog().expect("catalog");
    let engines = EngineSet::build(&catalog).expect("engines");
    let profile = reslot(catalog[0].profile.as_ref(), 0);
    let mut session = Session::new(profile, "http://127.0.0.1:9/");
    let r = fetch_page(&engines, 0, &mut session, "http://127.0.0.1:9/").await;
    let err = r.err().expect("refused");
    assert!(
        err.to_string().contains("transport") || err.to_string().contains("os error 111"),
        "unexpected: {err}"
    );
}

#[tokio::test]
async fn live_example_com_parses_in_single_pass() {
    let catalog = engine_catalog().expect("catalog");
    let engines = EngineSet::build(&catalog).expect("engines");
    let profile = reslot(catalog[0].profile.as_ref(), 0);
    let mut session = Session::new(profile, "https://example.com");
    let f = tokio::time::timeout(
        Duration::from_secs(30),
        fetch_page(&engines, 0, &mut session, "https://example.com"),
    )
    .await
    .expect("deadline")
    .expect("fetch ok");
    assert_eq!(f.status, 200);
    assert_eq!(f.page.title.as_deref(), Some("Example Domain"));
    assert!(f.bytes_in > 500);
    assert_eq!(f.page.utf8_bad_chunks, 0);
    assert!(!f.page.truncated);
}

#[test]
fn catalog_is_coherent_per_entry() {
    let catalog = engine_catalog().expect("catalog");
    assert!(catalog.len() >= 6);
    for entry in &catalog {
        let p: &Profile = entry.profile.as_ref();
        assert!(p.validate().is_ok(), "entry must be coherent");
        assert!(p.ua.starts_with("Mozilla/5.0"));
    }
    let mut seen = std::collections::HashSet::new();
    for entry in &catalog {
        assert!(seen.insert(entry.profile.ua.to_string()), "uas must differ");
    }
}
