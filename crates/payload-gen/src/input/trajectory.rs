use crate::input::event::{RawEvent, input};
use crate::input::motion::MotionCursor;
use crate::input::persona::Persona;
use crate::input::prng::SplitMix64Rng;
use crate::input::windmouse::{WindMouseParams, wind_mouse};

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TrajectoryPoint {
    pub x: i32,
    pub y: i32,
    pub dt_ms: u32,
}

#[derive(Debug, Clone)]
pub struct TrajectoryParams {
    pub fitts_a: f64,
    pub fitts_b: f64,
    pub wind: WindMouseParams,
    pub tremor_amplitude: f64,
    pub tremor_frequency: f64,
    pub overshoot_probability: f64,
    pub min_points: usize,
    pub max_points: usize,
}

impl Default for TrajectoryParams {
    fn default() -> Self {
        Self {
            fitts_a: 0.230,
            fitts_b: 0.166,
            wind: WindMouseParams::default(),
            tremor_amplitude: 1.5,
            tremor_frequency: 8.0,
            overshoot_probability: 0.3,
            min_points: 20,
            max_points: 500,
        }
    }
}

pub struct Trajectory {
    pub points: Vec<TrajectoryPoint>,
}

impl Trajectory {
    pub fn generate(
        start_x: f64,
        start_y: f64,
        dest_x: f64,
        dest_y: f64,
        target_width: f64,
        seed: u32,
        params: &TrajectoryParams,
    ) -> Self {
        let distance = ((dest_x - start_x).powi(2) + (dest_y - start_y).powi(2)).sqrt();
        if distance < 1.0 {
            return Self {
                points: vec![TrajectoryPoint {
                    x: dest_x as i32,
                    y: dest_y as i32,
                    dt_ms: 0,
                }],
            };
        }
        let persona = Persona {
            poll_hz: 125,
            fitts_a_ms: params.fitts_a * 1000.0,
            fitts_b_ms: params.fitts_b * 1000.0,
            gravity: params.wind.gravity,
            wind: params.wind.wind,
            max_step: params.wind.max_step,
            wind_jitter_ln: 0.18,
            tremor_amp_px: params.tremor_amplitude,
            tremor_hz: params.tremor_frequency,
            overshoot_p: params.overshoot_probability,
            corrections_max: 3,
            dwell_median_ms: 95.0,
            dwell_sigma_ln: 0.27,
            wpm: 55.0,
            typing_sigma_ln: 0.32,
            burst_len: 5,
            burst_pause_median_ms: 240.0,
            backspace_p: 0.04,
            caps_error_p: 0.02,
            notch_px: 110.0,
            notch_friction: 0.86,
            notch_gap_median_ms: 110.0,
            overscroll_p: 0.08,
            reading_pause_p: 0.10,
            double_click_p: 0.05,
            right_click_p: 0.03,
            hover_p: 0.85,
        };
        let mut cursor = MotionCursor::new(
            persona,
            start_x,
            start_y,
            dest_x,
            dest_y,
            target_width.max(4.0),
            0,
            seed as u64 ^ 0x7E11_5EED_0000_0000,
            0,
        );
        let mut points: Vec<TrajectoryPoint> = Vec::with_capacity(params.min_points.max(32));
        let mut now = 0u64;
        while !cursor.done() && now < 120_000_000 && points.len() < params.max_points {
            if let Some(ev) = cursor.step(now) {
                points.push(TrajectoryPoint {
                    x: ev.x as i32,
                    y: ev.y as i32,
                    dt_ms: ev.dt_ms as u32,
                });
            }
            now = cursor.next_due_us().max(now + 1);
        }
        if points.is_empty() {
            points.push(TrajectoryPoint {
                x: dest_x as i32,
                y: dest_y as i32,
                dt_ms: 0,
            });
        }
        points[0].dt_ms = 0;
        let last = points.len() - 1;
        points[last].x = dest_x as i32;
        points[last].y = dest_y as i32;
        Self { points }
    }

    pub fn total_duration_ms(&self) -> u32 {
        self.points.iter().map(|p| p.dt_ms).sum()
    }

    #[inline]
    pub fn len(&self) -> usize {
        self.points.len()
    }

    #[inline]
    pub fn is_empty(&self) -> bool {
        self.points.is_empty()
    }

    pub fn iter(&self) -> impl Iterator<Item = (i32, i32, u32)> + '_ {
        self.points.iter().map(|p| (p.x, p.y, p.dt_ms))
    }
}

pub fn generate_idle_drift(
    current_x: f64,
    current_y: f64,
    seed: u32,
    count: usize,
) -> Vec<TrajectoryPoint> {
    let mut rng = SplitMix64Rng::new(seed as u64 ^ 0xDDDD_CCCC_BBBB_AAAA);
    let perlin = crate::input::perlin::Perlin2D::from_seed(seed as u64 ^ 0x1111_2222_3333_4444);
    let mut points = Vec::with_capacity(count);
    for i in 0..count {
        let t = i as f64;
        let px = perlin.noise(t * 0.3, 0.0) * 0.8;
        let py = perlin.noise(0.0, t * 0.3) * 0.8;
        let jx = (rng.next_f64() - 0.5) * 0.4;
        let jy = (rng.next_f64() - 0.5) * 0.4;
        let dt = 2000u32 + (rng.next_f64() * 3000.0) as u32;
        points.push(TrajectoryPoint {
            x: (current_x + px + jx).round() as i32,
            y: (current_y + py + jy).round() as i32,
            dt_ms: dt,
        });
    }
    points
}

pub fn generate_heartbeat_bump(seed: u64, t_ms: u64, bump_px: i32) -> RawEvent {
    let mut rng = SplitMix64Rng::new(seed).stepped(t_ms);
    let dx = rng.next_range(0, bump_px as u32 * 2 + 1) as i32 - bump_px;
    let dy = rng.next_range(0, bump_px as u32 * 2 + 1) as i32 - bump_px;
    RawEvent::new(
        dx.unsigned_abs() as u16,
        dy.unsigned_abs() as u16,
        0,
        input::MOVE,
        0,
    )
}

pub fn base_path_windmouse(
    start: (f64, f64),
    dest: (f64, f64),
    params: WindMouseParams,
    seed: u64,
) -> Vec<(i32, i32)> {
    let mut rng = SplitMix64Rng::new(seed);
    let mut out = Vec::with_capacity(200);
    wind_mouse(start.0, start.1, dest.0, dest.1, params, &mut || rng.next_f64(), &mut out);
    out
}
