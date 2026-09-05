use crate::prng::SplitMix64Rng;

#[derive(Debug, Clone, Copy)]
pub struct Persona {
    pub poll_hz: u32,
    pub fitts_a_ms: f64,
    pub fitts_b_ms: f64,
    pub gravity: f64,
    pub wind: f64,
    pub max_step: f64,
    pub wind_jitter_ln: f64,
    pub tremor_amp_px: f64,
    pub tremor_hz: f64,
    pub overshoot_p: f64,
    pub corrections_max: u8,
    pub dwell_median_ms: f64,
    pub dwell_sigma_ln: f64,
    pub wpm: f64,
    pub typing_sigma_ln: f64,
    pub burst_len: u32,
    pub burst_pause_median_ms: f64,
    pub backspace_p: f64,
    pub caps_error_p: f64,
    pub notch_px: f64,
    pub notch_friction: f64,
    pub notch_gap_median_ms: f64,
    pub overscroll_p: f64,
    pub reading_pause_p: f64,
    pub double_click_p: f64,
    pub right_click_p: f64,
    pub hover_p: f64,
}

impl Persona {
    pub fn derive(farble_seed: u64, ctx_id: u64) -> Self {
        let mut rng = SplitMix64Rng::new(farble_seed ^ ctx_id.wrapping_mul(0x9E3779B97F4A7C15));
        let poll_class = rng.next_u64() % 100;
        let poll_hz = if poll_class < 60 {
            125
        } else if poll_class < 92 {
            500
        } else {
            1000
        };
        let wpm = 28.0 + rng.next_f64() * 62.0;
        Self {
            poll_hz,
            fitts_a_ms: 140.0 + rng.next_f64() * 160.0,
            fitts_b_ms: 90.0 + rng.next_f64() * 130.0,
            gravity: 6.5 + rng.next_f64() * 5.5,
            wind: 2.0 + rng.next_f64() * 3.5,
            max_step: 10.0 + rng.next_f64() * 12.0,
            wind_jitter_ln: 0.12 + rng.next_f64() * 0.16,
            tremor_amp_px: 0.25 + rng.next_f64() * 0.9,
            tremor_hz: 7.5 + rng.next_f64() * 4.5,
            overshoot_p: 0.10 + rng.next_f64() * 0.30,
            corrections_max: 1 + (rng.next_u64() % 3) as u8,
            dwell_median_ms: 78.0 + rng.next_f64() * 34.0,
            dwell_sigma_ln: 0.22 + rng.next_f64() * 0.10,
            wpm,
            typing_sigma_ln: 0.28 + rng.next_f64() * 0.14,
            burst_len: 3 + (rng.next_u64() % 5) as u32,
            burst_pause_median_ms: 180.0 + rng.next_f64() * 240.0,
            backspace_p: 0.02 + rng.next_f64() * 0.06,
            caps_error_p: 0.01 + rng.next_f64() * 0.05,
            notch_px: 90.0 + rng.next_f64() * 55.0,
            notch_friction: 0.82 + rng.next_f64() * 0.11,
            notch_gap_median_ms: 70.0 + rng.next_f64() * 90.0,
            overscroll_p: 0.05 + rng.next_f64() * 0.12,
            reading_pause_p: 0.08 + rng.next_f64() * 0.12,
            double_click_p: 0.03 + rng.next_f64() * 0.07,
            right_click_p: 0.02 + rng.next_f64() * 0.04,
            hover_p: 0.70 + rng.next_f64() * 0.28,
        }
    }

    #[inline]
    pub fn movement_time_ms(&self, distance_px: f64, target_w_px: f64, rng: &mut SplitMix64Rng) -> u32 {
        let w = target_w_px.max(4.0);
        let id = (distance_px / w).sqrt() + 0.5 * (distance_px / w + 1.0).log2();
        let base = self.fitts_a_ms + self.fitts_b_ms * id;
        rng.lognormal_ms(base, 0.18)
    }

    #[inline]
    pub fn throttle(&self, trust: i32) -> ThrottledPersona {
        let cool = trust.min(0).unsigned_abs() as f64 * 0.25;
        ThrottledPersona {
            overshoot_p: (self.overshoot_p * (1.0 + cool)).min(0.9),
            corrections_max: (self.corrections_max + cool as u8).min(4),
            dwell_sigma_ln: (self.dwell_sigma_ln * (1.0 + cool * 0.5)).min(0.6),
            burst_len: (self.burst_len.saturating_sub(cool as u32)).max(2),
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct ThrottledPersona {
    pub overshoot_p: f64,
    pub corrections_max: u8,
    pub dwell_sigma_ln: f64,
    pub burst_len: u32,
}
