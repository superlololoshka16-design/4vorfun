use crate::event::{RawEvent, button, input};
use crate::perlin::Perlin2D;
use crate::persona::Persona;
use crate::prng::SplitMix64Rng;

const OVERSHOOT_MIN_PX: f64 = 200.0;
const ARRIVE_EPS_PX: f64 = 0.75;
const SQRT3: f64 = 1.7320508075688772;
const SQRT5: f64 = 2.2360679774997896;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum State {
    Moving,
    Dwelling,
    Done,
}

pub struct MotionCursor {
    persona: Persona,
    x: f64,
    y: f64,
    vx: f64,
    vy: f64,
    wind_x: f64,
    wind_y: f64,
    max_step: f64,
    gravity: f64,
    wind: f64,
    aim_x: f64,
    aim_y: f64,
    target_x: f64,
    target_y: f64,
    target_w: f64,
    corrections_left: u8,
    deadline_us: u64,
    next_due_us: u64,
    quantum_us: u64,
    state: State,
    dwell_until_us: u64,
    rng: SplitMix64Rng,
    tremor: Perlin2D,
    last_emit_x: i32,
    last_emit_y: i32,
    last_emit_us: u64,
}

impl MotionCursor {
    pub fn new(
        persona: Persona,
        from_x: f64,
        from_y: f64,
        to_x: f64,
        to_y: f64,
        target_w: f64,
        now_us: u64,
        seed: u64,
        trust: i32,
    ) -> Self {
        let mut rng = SplitMix64Rng::new(seed ^ 0x1D2B_A3C4_5E6F_7081);
        let dist = ((to_x - from_x).powi(2) + (to_y - from_y).powi(2)).sqrt();
        let budget_ms = persona.movement_time_ms(dist, target_w, &mut rng) as u64;
        let throttle = persona.throttle(trust);
        let aim = if dist > OVERSHOOT_MIN_PX && rng.chance(throttle.overshoot_p) {
            let dir_x = (to_x - from_x) / dist;
            let dir_y = (to_y - from_y) / dist;
            let beyond = dist * (0.06 + rng.next_f64() * 0.14);
            let perp = (rng.next_f64() - 0.5) * dist * 0.08;
            (
                to_x + dir_x * beyond - dir_y * perp,
                to_y + dir_y * beyond + dir_x * perp,
            )
        } else {
            (to_x, to_y)
        };
        let mut c = Self {
            persona,
            x: from_x,
            y: from_y,
            vx: 0.0,
            vy: 0.0,
            wind_x: 0.0,
            wind_y: 0.0,
            max_step: persona.max_step,
            gravity: persona.gravity * (0.85 + rng.next_f64() * 0.3),
            wind: persona.wind * (0.8 + rng.next_f64() * 0.4),
            aim_x: aim.0,
            aim_y: aim.1,
            target_x: to_x,
            target_y: to_y,
            target_w: target_w.max(4.0),
            corrections_left: throttle.corrections_max,
            deadline_us: now_us + budget_ms * 1000,
            next_due_us: now_us,
            quantum_us: 1_000_000 / persona.poll_hz.max(1) as u64,
            state: State::Moving,
            dwell_until_us: 0,
            rng,
            tremor: Perlin2D::from_seed(seed ^ 0x7E11_0000_C0FF_EEEE),
            last_emit_x: from_x.round() as i32,
            last_emit_y: from_y.round() as i32,
            last_emit_us: now_us,
        };
        c.rng.next_u64();
        c
    }

    #[inline]
    pub fn done(&self) -> bool {
        self.state == State::Done
    }

    #[inline]
    pub fn pos(&self) -> (i32, i32) {
        (self.x.round() as i32, self.y.round() as i32)
    }

    #[inline]
    pub fn next_due_us(&self) -> u64 {
        self.next_due_us
    }

    #[inline]
    pub fn on_target(&self) -> bool {
        let dx = self.x - self.target_x;
        let dy = self.y - self.target_y;
        (dx * dx + dy * dy).sqrt() <= self.target_w * 0.5
    }

    pub fn step(&mut self, now_us: u64) -> Option<RawEvent> {
        if self.state == State::Done || now_us < self.next_due_us {
            return None;
        }
        self.next_due_us = now_us + self.quantum_us;
        match self.state {
            State::Moving => self.step_moving(now_us),
            State::Dwelling => self.step_dwelling(now_us),
            State::Done => None,
        }
    }

    fn step_moving(&mut self, now_us: u64) -> Option<RawEvent> {
        if self.arrived(now_us) {
            return None;
        }
        let dx = self.aim_x - self.x;
        let dy = self.aim_y - self.y;
        let dist = (dx * dx + dy * dy).sqrt();
        if dist >= 1.0 {
            let wind_mag = self.wind.min(dist);
            if dist >= self.persona.max_step * 0.8 {
                self.wind_x = self.wind_x / SQRT3
                    + (2.0 * self.rng.next_f64() - 1.0) * wind_mag / SQRT5;
                self.wind_y = self.wind_y / SQRT3
                    + (2.0 * self.rng.next_f64() - 1.0) * wind_mag / SQRT5;
            } else {
                self.wind_x /= SQRT3;
                self.wind_y /= SQRT3;
                if self.max_step < 3.0 {
                    self.max_step = self.rng.next_f64() * 3.0 + 3.0;
                } else {
                    self.max_step /= SQRT5;
                }
            }
            self.vx += self.wind_x + self.gravity * dx / dist;
            self.vy += self.wind_y + self.gravity * dy / dist;
            let v_mag = (self.vx * self.vx + self.vy * self.vy).sqrt();
            if v_mag > self.max_step {
                let clip = self.max_step * (0.5 + self.rng.next_f64() * 0.5);
                self.vx = (self.vx / v_mag) * clip;
                self.vy = (self.vy / v_mag) * clip;
            }
            self.x += self.vx;
            self.y += self.vy;
        }
        let speed = (self.vx * self.vx + self.vy * self.vy).sqrt();
        let t_s = now_us as f64 / 1e6;
        let amp = self.persona.tremor_amp_px * (0.4 + 3.0 / (1.0 + speed));
        let tx = self.tremor.noise(t_s * self.persona.tremor_hz, 0.0) * amp;
        let ty = self.tremor.noise(0.0, t_s * self.persona.tremor_hz) * amp;
        let ex = (self.x + tx).round() as i32;
        let ey = (self.y + ty).round() as i32;
        if ex == self.last_emit_x && ey == self.last_emit_y {
            return None;
        }
        let dt = (((now_us - self.last_emit_us) / 1000).max(1)).min(u16::MAX as u64) as u16;
        self.last_emit_x = ex;
        self.last_emit_y = ey;
        self.last_emit_us = now_us;
        Some(RawEvent::new(
            ex.clamp(0, u16::MAX as i32) as u16,
            ey.clamp(0, u16::MAX as i32) as u16,
            dt,
            input::MOVE,
            button::LEFT,
        ))
    }

    fn arrived(&mut self, now_us: u64) -> bool {
        let to_aim = ((self.aim_x - self.x).powi(2) + (self.aim_y - self.y).powi(2)).sqrt();
        let deadline = now_us >= self.deadline_us;
        if to_aim <= ARRIVE_EPS_PX || deadline {
            if (self.aim_x - self.target_x).abs() < 1e-9
                && (self.aim_y - self.target_y).abs() < 1e-9
            {
                self.enter_dwell(now_us);
            } else if self.corrections_left > 0 {
                self.corrections_left -= 1;
                self.aim_x = self.target_x;
                self.aim_y = self.target_y;
                self.max_step = (self.max_step * 0.55).max(2.5);
                self.gravity *= 1.3;
                self.wind *= 0.5;
                if deadline {
                    self.deadline_us = now_us + 120_000;
                }
            } else {
                self.enter_dwell(now_us);
            }
            return true;
        }
        false
    }

    fn enter_dwell(&mut self, now_us: u64) {
        let dwell = self
            .rng
            .lognormal_ms(self.persona.dwell_median_ms, self.persona.dwell_sigma_ln);
        self.dwell_until_us = now_us + dwell as u64 * 1000;
        self.state = State::Dwelling;
        self.next_due_us = now_us + self.quantum_us;
    }

    fn step_dwelling(&mut self, now_us: u64) -> Option<RawEvent> {
        if now_us >= self.dwell_until_us {
            self.state = State::Done;
            return None;
        }
        let t_s = now_us as f64 / 1e6;
        let press_wobble = 1.6;
        let tx = self.tremor.noise(t_s * self.persona.tremor_hz * 1.7, 50.0) * press_wobble;
        let ty = self.tremor.noise(50.0, t_s * self.persona.tremor_hz * 1.7) * press_wobble;
        let ex = (self.x + tx).round() as i32;
        let ey = (self.y + ty).round() as i32;
        if ex == self.last_emit_x && ey == self.last_emit_y {
            return None;
        }
        let dt = (((now_us - self.last_emit_us) / 1000).max(1)).min(u16::MAX as u64) as u16;
        self.last_emit_x = ex;
        self.last_emit_y = ey;
        self.last_emit_us = now_us;
        Some(RawEvent::new(
            ex.clamp(0, u16::MAX as i32) as u16,
            ey.clamp(0, u16::MAX as i32) as u16,
            dt,
            input::MOVE,
            button::LEFT,
        ))
    }
}
