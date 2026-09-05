use crate::event::{RawEvent, input};
use crate::persona::Persona;
use crate::prng::SplitMix64Rng;

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
