use payload_gen::{
    Rng, asn_info, canvas_hash, jitter_ratio, md5_hex_into, pick_profile, pow_search,
    sha256_hex_into, webgl_param, xxh3, xxh3_seeded,
};
use session_state::{Family, NetKind, Platform, Profile};
use std::sync::Arc;

#[test]
fn sha256_known_vector() {
    let mut out = [0u8; 64];
    sha256_hex_into(b"abc", &mut out);
    assert_eq!(
        &out,
        b"ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
    );
}

#[test]
fn md5_known_vector() {
    let mut out = [0u8; 32];
    md5_hex_into(b"abc", &mut out);
    assert_eq!(&out, b"900150983cd24fb0d6963f7d28e17f72");
}

#[test]
fn pow_search_finds_verifiable_nonce() {
    let sol = pow_search(b"silo-pow:", 12, 5_000_000).expect("pow solvable");
    let mut scratch = b"silo-pow:".to_vec();
    scratch.extend_from_slice(&sol.nonce.to_le_bytes());
    let mut hex = [0u8; 64];
    sha256_hex_into(&scratch, &mut hex);
    let mut zeros = 0u32;
    for b in sol.digest {
        if b == 0 {
            zeros += 8;
        } else {
            zeros += b.leading_zeros();
            break;
        }
    }
    assert!(zeros >= 12, "digest must carry {zeros} leading zero bits");
}

#[test]
fn pow_search_respects_iteration_deadline() {
    assert!(pow_search(b"x", 40, 10).is_none());
}

#[test]
fn canvas_hash_deterministic_and_profile_sensitive() {
    let a = canvas_hash(7, "Google Inc.", "ANGLE (NVIDIA)");
    let b = canvas_hash(7, "Google Inc.", "ANGLE (NVIDIA)");
    let c = canvas_hash(8, "Google Inc.", "ANGLE (NVIDIA)");
    assert_eq!(a, b);
    assert_ne!(a, c);
    assert!(webgl_param(7, 0) >= 1.0 && webgl_param(7, 0) <= 2048.0);
    assert_ne!(webgl_param(7, 0), webgl_param(9, 0));
}

#[test]
fn jitter_stays_in_band() {
    let mut rng = Rng::new(0xC0FFEE);
    let median = 120.0f64;
    for _ in 0..2000 {
        let j = jitter_ratio(median, &mut rng);
        assert!(
            j >= median * 0.55 && j <= median * 1.6,
            "jitter out of band: {j}"
        );
    }
}

#[test]
fn xxh3_stable_and_seeded() {
    assert_eq!(xxh3(b"silo"), xxh3(b"silo"));
    assert_ne!(xxh3(b"silo"), xxh3(b"siol"));
    assert_ne!(xxh3_seeded(1, b"silo"), xxh3_seeded(2, b"silo"));
}

#[test]
fn asn_table_maps_kinds() {
    assert_eq!(asn_info(15169).net, NetKind::Datacenter);
    assert_eq!(asn_info(7922).net, NetKind::Residential);
    assert_eq!(asn_info(7018).net, NetKind::Mobile);
    assert_eq!(asn_info(999999).net, NetKind::Residential);
    assert_eq!(asn_info(3209).locale, "de-DE");
}

fn profile(net: NetKind) -> Arc<Profile> {
    Arc::new(Profile {
        ua: Arc::from("Mozilla/5.0 (Windows NT 10.0; Win64; x64) Chrome/149.0.0.0 Safari/537.36"),
        sec_ch_ua: Arc::from(r#""Google Chrome";v="149""#),
        accept_language: Arc::from("en-US"),
        platform: Platform::Windows,
        locale: "en-US".into(),
        tz: "America/New_York".into(),
        screen_w: 1920,
        screen_h: 1080,
        canvas_seed: 1,
        webgl_vendor: "v".into(),
        webgl_renderer: "r".into(),
        asn: 0,
        net,
        family: Family::Chrome { major: 149 },
    })
}

#[test]
fn pick_profile_matches_net_kind() {
    let pool = vec![profile(NetKind::Datacenter), profile(NetKind::Mobile)];
    let mut rng = Rng::new(3);
    for asn in [15169u32, 16509, 9009] {
        let p = pick_profile(&pool, asn, &mut rng).expect("non-empty pool");
        assert_eq!(p.net, NetKind::Datacenter);
    }
    let p = pick_profile(&pool, 7018, &mut rng).expect("non-empty pool");
    assert_eq!(p.net, NetKind::Mobile);
    assert!(pick_profile(&[], 15169, &mut rng).is_none());
}

#[test]
fn rng_distribution_sane() {
    let mut rng = Rng::new(7);
    let mut ones = 0u32;
    for _ in 0..100_000 {
        if rng.next_f64() < 0.5 {
            ones += 1;
        }
    }
    let ratio = ones as f64 / 100_000.0;
    assert!((ratio - 0.5).abs() < 0.02, "coin flip ratio {ratio}");
    let g: f64 = (0..10_000).map(|_| rng.gauss()).sum::<f64>() / 10_000.0;
    assert!(g.abs() < 0.1, "gauss mean drifted: {g}");
}
