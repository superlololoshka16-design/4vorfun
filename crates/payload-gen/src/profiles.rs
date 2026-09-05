use crate::rng::Rng;
use session_state::NetKind;
use std::sync::Arc;

pub struct AsnInfo {
    pub carrier: &'static str,
    pub net: NetKind,
    pub tz: &'static str,
    pub locale: &'static str,
}

static ASN_TABLE: phf::Map<u32, (&'static str, u8, &'static str, &'static str)> = phf::phf_map! {
    15169u32 => ("Google LLC", 0u8, "America/Los_Angeles", "en-US"),
    8075u32 => ("Microsoft Corporation", 0u8, "America/Chicago", "en-US"),
    16509u32 => ("Amazon.com", 0u8, "America/New_York", "en-US"),
    14061u32 => ("DigitalOcean LLC", 0u8, "America/New_York", "en-US"),
    24940u32 => ("Hetzner Online GmbH", 0u8, "Europe/Berlin", "de-DE"),
    9009u32 => ("M247 Europe SRL", 0u8, "Europe/Bucharest", "ro-RO"),
    7922u32 => ("Comcast Cable", 1u8, "America/Denver", "en-US"),
    3320u32 => ("Deutsche Telekom AG", 1u8, "Europe/Berlin", "de-DE"),
    20057u32 => ("AT&T Internet", 1u8, "America/Chicago", "en-US"),
    7843u32 => ("Charter Communications", 1u8, "America/Los_Angeles", "en-US"),
    22394u32 => ("Comcast Business", 1u8, "America/New_York", "en-US"),
    7018u32 => ("AT&T Mobility", 2u8, "America/Chicago", "en-US"),
    21930u32 => ("T-Mobile USA", 2u8, "America/New_York", "en-US"),
    3209u32 => ("Vodafone GmbH", 2u8, "Europe/Berlin", "de-DE"),
    22201u32 => ("Vodafone Italia", 2u8, "Europe/Rome", "it-IT"),
    4400u32 => ("KDDI Corporation", 2u8, "Asia/Tokyo", "ja-JP"),
    701u32 => ("Verizon Wireless", 2u8, "America/New_York", "en-US"),
};

fn kind_from(code: u8) -> NetKind {
    match code {
        0 => NetKind::Datacenter,
        2 => NetKind::Mobile,
        _ => NetKind::Residential,
    }
}

pub fn asn_info(asn: u32) -> AsnInfo {
    match ASN_TABLE.get(&asn) {
        Some((carrier, code, tz, locale)) => AsnInfo {
            carrier,
            net: kind_from(*code),
            tz,
            locale,
        },
        None => AsnInfo {
            carrier: "unknown",
            net: NetKind::Residential,
            tz: "America/New_York",
            locale: "en-US",
        },
    }
}

pub fn pick_profile<'a>(
    pool: &'a [Arc<session_state::Profile>],
    asn: u32,
    rng: &mut Rng,
) -> Option<&'a Arc<session_state::Profile>> {
    if pool.is_empty() {
        return None;
    }
    let want = asn_info(asn).net;
    let mut idx = rng.below(pool.len());
    for _ in 0..pool.len() {
        if pool[idx].net == want {
            return Some(&pool[idx]);
        }
        idx = (idx + 1) % pool.len();
    }
    Some(&pool[rng.below(pool.len())])
}
