use crate::input::prng::SplitMix64Rng;
use crate::input::windmouse::{WindMouseParams, wind_mouse};

pub const WINDMOUSE_GRAVITY: f64 = 9.0;
pub const WINDMOUSE_WIND: f64 = 3.0;
pub const WINDMOUSE_MAX_STEP: f64 = 15.0;
pub const WINDMOUSE_DAMPED: f64 = 12.0;
pub const HAND_TREMOR_FREQ_LO: f64 = 8.0;
pub const HAND_TREMOR_FREQ_HI: f64 = 12.0;
pub const HAND_TREMOR_AMP_LO: f64 = 0.3;
pub const HAND_TREMOR_AMP_HI: f64 = 1.2;
pub const COGNITIVE_DELAY_LO_MS: u32 = 200;
pub const COGNITIVE_DELAY_HI_MS: u32 = 500;
pub const HEARTBEAT_INTERVAL_MIN_MS: u32 = 4_000;
pub const HEARTBEAT_INTERVAL_MAX_MS: u32 = 12_000;
pub const HEARTBEAT_BUMP_PX: i32 = 12;
pub const VISIBILITY_FLIP_INTERVAL_MS: u64 = 15 * 60 * 1_000;
pub const VISIBILITY_FLIP_JITTER_MS: u64 = 60_000;
pub const REQUEST_JITTER_MAX_MS: u32 = 500;

pub const IK_KEY_DOWN: u8 = 0;
pub const IK_MOUSE_MOVE: u8 = 1;
pub const IK_CLICK: u8 = 2;
pub const IK_SCROLL: u8 = 3;
pub const VS_VISIBLE: u8 = 0;
pub const VS_HIDDEN: u8 = 1;

#[inline]
pub const fn per_tab_seed(farble_seed: u64, ctx_id: u64) -> u64 {
    farble_seed ^ ctx_id
}

#[inline]
pub const fn tab_rng(farble_seed: u64, ctx_id: u64) -> SplitMix64Rng {
    SplitMix64Rng::new(per_tab_seed(farble_seed, ctx_id))
}

#[derive(Debug, Clone)]
pub struct MouseHeartbeat {
    pub path: Vec<(i32, i32)>,
    pub next_interval_ms: u32,
}

pub fn mouse_heartbeat(farble_seed: u64, ctx_id: u64, t_ms: u64) -> MouseHeartbeat {
    let mut lcg = tab_rng(farble_seed, ctx_id).stepped(t_ms);
    let mag = HEARTBEAT_BUMP_PX as u32;
    let dx = lcg.next_range(0, mag * 2 + 1) as i32 - HEARTBEAT_BUMP_PX;
    let dy = lcg.next_range(0, mag * 2 + 1) as i32 - HEARTBEAT_BUMP_PX;
    let next_interval_ms = lcg.next_range(HEARTBEAT_INTERVAL_MIN_MS, HEARTBEAT_INTERVAL_MAX_MS + 1);
    let path = if dx == 0 && dy == 0 {
        Vec::default()
    } else {
        wind_mouse_path(0.0, 0.0, dx as f64, dy as f64, &mut lcg)
    };
    MouseHeartbeat { path, next_interval_ms }
}

pub fn wind_mouse_path(
    start_x: f64,
    start_y: f64,
    dest_x: f64,
    dest_y: f64,
    lcg: &mut SplitMix64Rng,
) -> Vec<(i32, i32)> {
    let mut out: Vec<(i32, i32)> = Vec::default();
    let params = WindMouseParams {
        gravity: WINDMOUSE_GRAVITY,
        wind: WINDMOUSE_WIND,
        max_step: WINDMOUSE_MAX_STEP,
        damped_distance: WINDMOUSE_DAMPED,
    };
    let mut rng = || lcg.next_f64();
    wind_mouse(start_x, start_y, dest_x, dest_y, params, &mut rng, &mut out);
    out
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VisibilityFlip {
    pub new_state: u8,
    pub next_flip_ms: u64,
}

#[inline]
pub const fn visibility_state_str(state: u8) -> &'static str {
    match state {
        VS_HIDDEN => "hidden",
        _ => "visible",
    }
}

pub fn visibility_flip(farble_seed: u64, ctx_id: u64, t_ms: u64) -> Option<VisibilityFlip> {
    let cycle_len = VISIBILITY_FLIP_INTERVAL_MS;
    let cycle_idx = t_ms / cycle_len;
    let mut lcg = tab_rng(farble_seed, ctx_id).stepped(cycle_idx);
    let jitter = (lcg.next_u64() % VISIBILITY_FLIP_JITTER_MS).max(1);
    let this_cycle_start = cycle_idx * cycle_len;
    let flip_at = this_cycle_start.saturating_add(jitter);
    if t_ms < flip_at || t_ms >= this_cycle_start.saturating_add(cycle_len) {
        return None;
    }
    let new_state = if cycle_idx % 2 == 0 { VS_HIDDEN } else { VS_VISIBLE };
    let next_flip_ms = this_cycle_start + cycle_len + lcg.next_u64() % VISIBILITY_FLIP_JITTER_MS;
    Some(VisibilityFlip { new_state, next_flip_ms })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RequestJitter {
    pub octaves: [u32; 8],
}

impl RequestJitter {
    #[inline]
    pub fn value_ms(&self) -> u32 {
        let mut sum = 0.0;
        let mut total = 0.0;
        for (i, &v) in self.octaves.iter().enumerate() {
            let w = 1.0 / (1u64 << (i + 1)) as f64;
            sum += w * (v as f64 / u32::MAX as f64);
            total += w;
        }
        let normalised = sum / total;
        (normalised * REQUEST_JITTER_MAX_MS as f64) as u32
    }
}

pub fn request_jitter(farble_seed: u64, ctx_id: u64, t_ms: u64) -> u32 {
    let mut lcg = tab_rng(farble_seed, ctx_id).stepped(t_ms);
    let mut sum: f64 = 0.0;
    let mut total: f64 = 0.0;
    for i in 0..8u32 {
        let w = 1.0 / (1u64 << (i + 1)) as f64;
        sum += w * (lcg.next_u32() as f64 / u32::MAX as f64);
        total += w;
    }
    let normalised = sum / total;
    (normalised * REQUEST_JITTER_MAX_MS as f64) as u32
}

pub fn wrist_pivot_arc(waypoints: &[(f64, f64)], amplitude_px: f64) -> Vec<(f64, f64)> {
    if waypoints.len() < 2 || amplitude_px.abs() < 1e-6 {
        return waypoints.to_vec();
    }
    let start = waypoints[0];
    let end = *waypoints.last().expect("invariant");
    let dx = end.0 - start.0;
    let dy = end.1 - start.1;
    let len = (dx * dx + dy * dy).sqrt();
    if len < 1e-6 {
        return waypoints.to_vec();
    }
    let perp_x = -dy / len;
    let perp_y = dx / len;
    let denom = (waypoints.len() - 1) as f64;
    waypoints
        .iter()
        .enumerate()
        .map(|(i, &(x, y))| {
            let progress = i as f64 / denom;
            let shift = amplitude_px * (core::f64::consts::PI * progress).sin();
            (x + perp_x * shift, y + perp_y * shift)
        })
        .collect()
}

pub fn hand_tremor(t_ms: f64, lcg: &mut SplitMix64Rng) -> (f64, f64) {
    let freq = HAND_TREMOR_FREQ_LO + lcg.next_f64() * (HAND_TREMOR_FREQ_HI - HAND_TREMOR_FREQ_LO);
    let amp = HAND_TREMOR_AMP_LO + lcg.next_f64() * (HAND_TREMOR_AMP_HI - HAND_TREMOR_AMP_LO);
    let phase = lcg.next_f64() * core::f64::consts::TAU;
    let angle = core::f64::consts::TAU * freq * (t_ms / 1000.0) + phase;
    (amp * angle.cos(), amp * angle.sin())
}

#[inline]
pub fn cognitive_delay_ms(lcg: &mut SplitMix64Rng) -> u32 {
    lcg.lognormal_ms(310.0, 0.22).clamp(COGNITIVE_DELAY_LO_MS, COGNITIVE_DELAY_HI_MS)
}

#[inline]
pub fn input_safe(document_visible: bool, focused_input: bool, kind: u8) -> bool {
    match kind {
        IK_KEY_DOWN | IK_CLICK => document_visible && focused_input,
        _ => document_visible,
    }
}
