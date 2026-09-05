use compact_str::CompactString;
use core_utils::xxh3;
use session_state::{Family, NetKind, Platform, Profile};
use std::sync::Arc;
use wreq::IntoEmulation as _;
use wreq::header::{ACCEPT_LANGUAGE, HeaderName, USER_AGENT};
use wreq_util::{Emulation, Platform as WPlatform, Profile as WProfile};

struct CatalogRow {
    profile: WProfile,
    platform: WPlatform,
    family: Family,
    net: NetKind,
    tz: &'static str,
    locale: &'static str,
    screen: (u32, u32),
    vendor: &'static str,
    renderer: &'static str,
}

const ROWS: &[CatalogRow] = &[
    CatalogRow {
        profile: WProfile::Chrome149,
        platform: WPlatform::Windows,
        family: Family::Chrome { major: 149 },
        net: NetKind::Residential,
        tz: "America/New_York",
        locale: "en-US",
        screen: (1920, 1080),
        vendor: "Google Inc. (NVIDIA)",
        renderer: "ANGLE (NVIDIA, NVIDIA GeForce RTX 3060 Direct3D11 vs_5_0 ps_5_0, D3D11)",
    },
    CatalogRow {
        profile: WProfile::Chrome148,
        platform: WPlatform::MacOS,
        family: Family::Chrome { major: 148 },
        net: NetKind::Residential,
        tz: "Europe/Berlin",
        locale: "de-DE",
        screen: (1728, 1117),
        vendor: "Apple",
        renderer: "ANGLE (Apple, ANGLE Metal Renderer: Apple M2, Unspecified Version)",
    },
    CatalogRow {
        profile: WProfile::Chrome147,
        platform: WPlatform::Linux,
        family: Family::Chrome { major: 147 },
        net: NetKind::Datacenter,
        tz: "Europe/Berlin",
        locale: "en-US",
        screen: (1920, 1080),
        vendor: "Google Inc. (Intel)",
        renderer: "ANGLE (Intel, Mesa Intel(R) UHD Graphics 630 (CML GT2), OpenGL 4.6 (Core Profile) Mesa 23.2.1)",
    },
    CatalogRow {
        profile: WProfile::Edge148,
        platform: WPlatform::Windows,
        family: Family::Edge { major: 148 },
        net: NetKind::Residential,
        tz: "America/Chicago",
        locale: "en-US",
        screen: (2560, 1440),
        vendor: "Google Inc. (AMD)",
        renderer: "ANGLE (AMD, AMD Radeon RX 6700 XT Direct3D11 vs_5_0 ps_5_0, D3D11)",
    },
    CatalogRow {
        profile: WProfile::Firefox151,
        platform: WPlatform::Windows,
        family: Family::Firefox { major: 151 },
        net: NetKind::Residential,
        tz: "America/Denver",
        locale: "en-US",
        screen: (1920, 1080),
        vendor: "Google Inc. (Intel)",
        renderer: "WebGL: Google Inc. (Intel) - ANGLE (Intel, Intel(R) UHD Graphics 630, OpenGL 4.5)",
    },
    CatalogRow {
        profile: WProfile::Safari26_2,
        platform: WPlatform::MacOS,
        family: Family::Safari { mobile: false },
        net: NetKind::Mobile,
        tz: "Europe/London",
        locale: "en-GB",
        screen: (2056, 1329),
        vendor: "Apple",
        renderer: "Apple GPU",
    },
    CatalogRow {
        profile: WProfile::Chrome149,
        platform: WPlatform::Android,
        family: Family::Chrome { major: 149 },
        net: NetKind::Mobile,
        tz: "America/Chicago",
        locale: "en-US",
        screen: (1080, 2400),
        vendor: "Qualcomm",
        renderer: "Adreno (TM) 740",
    },
    CatalogRow {
        profile: WProfile::SafariIos26_2,
        platform: WPlatform::IOS,
        family: Family::Safari { mobile: true },
        net: NetKind::Mobile,
        tz: "Europe/Rome",
        locale: "it-IT",
        screen: (1179, 2556),
        vendor: "Apple",
        renderer: "Apple GPU",
    },
];

fn platform_of(p: WPlatform) -> Platform {
    match p {
        WPlatform::Windows => Platform::Windows,
        WPlatform::MacOS => Platform::MacOS,
        WPlatform::Linux => Platform::Linux,
        WPlatform::Android => Platform::Android,
        WPlatform::IOS => Platform::IOS,
        _ => Platform::Linux,
    }
}

fn wplatform_str(p: WPlatform) -> &'static str {
    match p {
        WPlatform::Windows => "Windows",
        WPlatform::MacOS => "macOS",
        WPlatform::Linux => "Linux",
        WPlatform::Android => "Android",
        WPlatform::IOS => "iOS",
        _ => "unknown",
    }
}

fn build_profile(row: &CatalogRow, origin_seed: u64) -> Result<Arc<Profile>, String> {
    let emu = Emulation::builder()
        .profile(row.profile)
        .platform(row.platform)
        .build()
        .into_emulation();
    let headers = &emu.headers;
    let ua = headers
        .get(USER_AGENT)
        .and_then(|v| v.to_str().ok())
        .unwrap_or_default();
    let sec = headers
        .get(HeaderName::from_static("sec-ch-ua"))
        .and_then(|v| v.to_str().ok())
        .unwrap_or_default();
    let lang = headers
        .get(ACCEPT_LANGUAGE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("en-US,en;q=0.9");
    let platform_hdr = headers
        .get(HeaderName::from_static("sec-ch-ua-platform"))
        .and_then(|v| v.to_str().ok())
        .map(|v| CompactString::new(v.trim_matches('"')));
    let mapped = match platform_hdr.as_deref() {
        None => platform_of(row.platform),
        Some("Windows") => Platform::Windows,
        Some("macOS") => Platform::MacOS,
        Some("Linux") => Platform::Linux,
        Some("Android") => Platform::Android,
        Some("iOS") => Platform::IOS,
        Some(_) => return Err("unknown platform header".into()),
    };
    if wplatform_str(row.platform) != mapped.as_str() {
        return Err("platform header mismatch".into());
    }
    let profile = Profile {
        ua: Arc::from(ua),
        sec_ch_ua: Arc::from(sec),
        accept_language: Arc::from(lang),
        platform: mapped,
        locale: row.locale.into(),
        tz: row.tz.into(),
        screen_w: row.screen.0,
        screen_h: row.screen.1,
        canvas_seed: xxh3::hash(ua.as_bytes()) ^ origin_seed.rotate_left(23),
        webgl_vendor: row.vendor.into(),
        webgl_renderer: row.renderer.into(),
        asn: 0,
        net: row.net,
        family: row.family,
    };
    profile
        .validate()
        .map_err(|e| e.to_string())?;
    Ok(Arc::new(profile))
}

pub struct CatalogEntry {
    pub profile: Arc<Profile>,
    pub wprofile: WProfile,
    pub wplatform: WPlatform,
}

pub fn engine_catalog() -> Result<Vec<CatalogEntry>, String> {
    let mut out = Vec::with_capacity(ROWS.len());
    for row in ROWS {
        out.push(CatalogEntry {
            profile: build_profile(row, 0)?,
            wprofile: row.profile,
            wplatform: row.platform,
        });
    }
    Ok(out)
}

pub fn reslot(profile: &Profile, asn: u32) -> Arc<Profile> {
    let mut next = profile.clone();
    next.canvas_seed = xxh3::hash(next.ua.as_bytes()) ^ (asn as u64).rotate_left(23);
    next.asn = asn;
    Arc::new(next)
}
