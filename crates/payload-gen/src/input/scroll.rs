use crate::input::event::{RawEvent, input};
use crate::input::persona::Persona;
use crate::input::prng::SplitMix64Rng;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScrollMethod {
    WheelNotch,
    Trackpad,
    Keyboard,
}

#[derive(Debug, Clone, Copy)]
pub struct ScrollStep {
    pub dy_px: i32,
    pub dt_ms: u32,
}

#[derive(Debug, Clone)]
pub struct ScrollPath {
    pub steps: Vec<ScrollStep>,
}

impl ScrollPath {
    pub fn total_px(&self) -> i32 {
        self.steps.iter().map(|s| s.dy_px).sum()
    }

    pub fn duration_ms(&self) -> u32 {
        self.steps.iter().map(|s| s.dt_ms).sum()
    }
}

#[inline]
pub fn chunk_size_px(persona: &Persona, rng: &mut SplitMix64Rng) -> i32 {
    let notches = 1 + rng.next_u64() % 3;
    (persona.notch_px * notches as f64) as i32
}

#[inline]
pub fn reading_scroll_duration_ms(rng: &mut SplitMix64Rng) -> u32 {
    rng.lognormal_ms(420.0, 0.35)
}

#[inline]
pub fn scroll_velocity_px_s(persona: &Persona, velocity_px_per_frame: f64) -> f64 {
    velocity_px_per_frame * 60.0 / persona.notch_friction
}

#[inline]
pub fn should_overscroll(persona: &Persona, rng: &mut SplitMix64Rng) -> bool {
    rng.chance(persona.overscroll_p)
}

#[inline]
pub fn should_overshoot(persona: &Persona, rng: &mut SplitMix64Rng) -> bool {
    rng.chance(persona.overscroll_p * 0.6)
}

pub fn generate_scroll_path(
    persona: Persona,
    total_px: f64,
    seed: u64,
    trust: i32,
) -> ScrollPath {
    let mut cur = ScrollCursor::new(persona, total_px, 0, seed, trust);
    let mut steps = Vec::new();
    let mut now = 0u64;
    let mut last_y = 0i32;
    while !cur.done() && now < 60_000_000 {
        if let Some(ev) = cur.step(now) {
            let y = ev.y as i32;
            steps.push(ScrollStep {
                dy_px: y - last_y,
                dt_ms: ev.dt_ms as u32,
            });
            last_y = y;
        }
        now = cur.next_due_us().max(now + 1);
    }
    ScrollPath { steps }
}

pub struct ScrollCursor {
    persona: Persona,
    velocity: f64,
    remaining_px: f64,
    frame_due_us: u64,
    notch_due_us: u64,
    finished: bool,
    rng: SplitMix64Rng,
    last_emit_y: i32,
    last_emit_us: u64,
    y: f64,
}

const FRAME_US: u64 = 16_667;

impl ScrollCursor {
    pub fn new(
        persona: Persona,
        total_px: f64,
        now_us: u64,
        seed: u64,
        trust: i32,
    ) -> Self {
        let mut rng = SplitMix64Rng::new(seed ^ 0x5C20_0000_0000_0000);
        let _ = trust;
        let overscroll = if rng.chance(persona.overscroll_p) {
            total_px.signum() * rng.next_f64() * persona.notch_px * 1.5
        } else {
            0.0
        };
        Self {
            persona,
            velocity: 0.0,
            remaining_px: total_px + overscroll,
            frame_due_us: now_us,
            notch_due_us: now_us,
            finished: false,
            rng,
            last_emit_y: 0,
            last_emit_us: now_us,
            y: 0.0,
        }
    }

    #[inline]
    pub fn done(&self) -> bool {
        self.finished
    }

    #[inline]
    pub fn next_due_us(&self) -> u64 {
        self.frame_due_us
    }

    #[inline]
    pub fn offset_px(&self) -> i32 {
        self.y.round() as i32
    }

    pub fn step(&mut self, now_us: u64) -> Option<RawEvent> {
        if self.finished || now_us < self.frame_due_us {
            return None;
        }
        self.frame_due_us = now_us + FRAME_US;
        if self.remaining_px != 0.0 {
            if now_us >= self.notch_due_us {
                let notches = 1 + self.rng.next_u64() % 3;
                let impulse = self.persona.notch_px * notches as f64 * self.remaining_px.signum();
                self.velocity += impulse;
                let gap = self.rng.lognormal_ms(self.persona.notch_gap_median_ms, 0.30);
                self.notch_due_us = now_us + gap as u64 * 1000;
            }
            self.velocity *= self.persona.notch_friction;
            let step = self.velocity;
            if self.remaining_px.abs() <= step.abs().max(1.0) {
                self.y += self.remaining_px;
                self.remaining_px = 0.0;
                self.velocity = 0.0;
                self.finished = true;
            } else {
                self.y += step;
                self.remaining_px -= step;
            }
        }
        let ey = self.y.round() as i32;
        if ey == self.last_emit_y {
            return None;
        }
        let dt = (((now_us - self.last_emit_us) / 1000).max(1)).min(u16::MAX as u64) as u16;
        self.last_emit_y = ey;
        self.last_emit_us = now_us;
        Some(RawEvent::new(0, ey.unsigned_abs() as u16, dt, input::WHEEL, 0))
    }
}
