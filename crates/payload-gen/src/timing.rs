use crate::rng::Rng;
use std::time::Duration;

pub fn jitter_ratio(median_ms: f64, rng: &mut Rng) -> f64 {
    let sigma = (median_ms * 0.22).clamp(median_ms * 0.15, median_ms * 0.30);
    let v = median_ms + rng.gauss() * sigma;
    v.clamp(median_ms * 0.55, median_ms * 1.60)
}

pub fn pace_delay(median_ms: f64, elapsed_ms: f64, rng: &mut Rng) -> Duration {
    let target = jitter_ratio(median_ms, rng);
    let left = target - elapsed_ms;
    if left <= 0.0 {
        Duration::from_millis(1)
    } else {
        Duration::from_secs_f64(left / 1000.0)
    }
}
