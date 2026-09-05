use crate::input::activity::{
    IK_MOUSE_MOVE, IK_SCROLL, mouse_heartbeat, visibility_flip, wind_mouse_path,
};
use crate::input::click::ClickCursor;
use crate::input::event::{RawEvent, button, input};
use crate::input::motion::MotionCursor;
use crate::input::persona::Persona;
use crate::input::prng::SplitMix64Rng;
use crate::input::scroll::ScrollCursor;
use crate::input::typing::TypingCursor;
use smallvec::SmallVec;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TabPhase {
    Approach,
    Click,
    Typing,
    Reading,
    Idle,
}

pub struct TabTick {
    pub events: SmallVec<[RawEvent; 32]>,
    pub next_due_us: u64,
    pub phase: TabPhase,
}

enum Active {
    Move(MotionCursor),
    Click(ClickCursor),
    Type(TypingCursor),
    Scroll(ScrollCursor),
    None,
}

pub struct TabSession {
    persona: Persona,
    phase: TabPhase,
    active: Active,
    x: f64,
    y: f64,
    text: Option<String>,
    seed: u64,
    ctx_id: u64,
    rng: SplitMix64Rng,
    idle_bump_path: Vec<(i32, i32)>,
    idle_bump_i: usize,
    next_heartbeat_us: u64,
    next_due_us: u64,
    finished: bool,
}

impl TabSession {
    pub fn start(
        persona: Persona,
        seed: u64,
        ctx_id: u64,
        from: (f64, f64),
        target: (f64, f64),
        target_w: f64,
        text: Option<String>,
        now_us: u64,
        trust: i32,
    ) -> Self {
        let move_cursor =
            MotionCursor::new(persona, from.0, from.1, target.0, target.1, target_w, now_us, seed, trust);
        Self {
            persona,
            phase: TabPhase::Approach,
            active: Active::Move(move_cursor),
            x: from.0,
            y: from.1,
            text,
            seed,
            ctx_id,
            rng: SplitMix64Rng::new(seed ^ 0x1DE1_0000_0000_0005),
            idle_bump_path: Vec::new(),
            idle_bump_i: 0,
            next_heartbeat_us: 0,
            next_due_us: now_us,
            finished: false,
        }
    }

    #[inline]
    pub fn phase(&self) -> TabPhase {
        self.phase
    }

    #[inline]
    pub fn finished(&self) -> bool {
        self.finished
    }

    #[inline]
    pub fn next_due_us(&self) -> u64 {
        self.next_due_us
    }

    pub fn advance(&mut self, now_us: u64) -> TabTick {
        let mut events: SmallVec<[RawEvent; 32]> = SmallVec::new();
        self.drive(now_us, &mut events);
        TabTick {
            events,
            next_due_us: self.next_due_us,
            phase: self.phase,
        }
    }

    fn drive(&mut self, now_us: u64, out: &mut SmallVec<[RawEvent; 32]>) {
        if self.finished || now_us < self.next_due_us {
            return;
        }
        match &mut self.active {
            Active::Move(c) => {
                if let Some(ev) = c.step(now_us) {
                    self.x = ev.x as f64;
                    self.y = ev.y as f64;
                    out.push(ev);
                }
                self.next_due_us = c.next_due_us();
                if c.done() {
                    self.enter_click(now_us);
                }
            }
            Active::Click(c) => {
                if let Some(ev) = c.step(now_us) {
                    out.push(ev);
                }
                self.next_due_us = c.next_due_us();
                if c.done() {
                    self.enter_next(now_us);
                }
            }
            Active::Type(c) => {
                if let Some(ev) = c.step(now_us) {
                    out.push(ev);
                }
                self.next_due_us = c.next_due_us();
                if c.done() {
                    self.enter_next(now_us);
                }
            }
            Active::Scroll(c) => {
                if let Some(ev) = c.step(now_us) {
                    out.push(ev);
                }
                self.next_due_us = c.next_due_us();
                if c.done() {
                    self.enter_idle(now_us);
                }
            }
            Active::None => self.idle_tick(now_us, out),
        }
    }

    fn enter_click(&mut self, now_us: u64) {
        self.phase = TabPhase::Click;
        let (x, y) = (self.x.clamp(0.0, u16::MAX as f64), self.y.clamp(0.0, u16::MAX as f64));
        self.active = Active::Click(ClickCursor::new(
            self.persona,
            x as u16,
            y as u16,
            now_us,
            self.seed ^ 0xC1C_0000_0000_0031,
            0,
        ));
        self.next_due_us = now_us;
    }

    fn enter_next(&mut self, now_us: u64) {
        match self.phase {
            TabPhase::Click => {
                if let Some(text) = self.text.take() {
                    self.phase = TabPhase::Typing;
                    self.active = Active::Type(TypingCursor::new(
                        self.persona,
                        &text,
                        now_us,
                        self.seed ^ 0x7E11_0000_0000_0037,
                        0,
                    ));
                    self.next_due_us = now_us;
                    return;
                }
                self.enter_reading(now_us);
            }
            TabPhase::Typing => self.enter_reading(now_us),
            _ => self.enter_idle(now_us),
        }
    }

    fn enter_reading(&mut self, now_us: u64) {
        self.phase = TabPhase::Reading;
        let scroll_px = -(320.0 + self.rng.next_f64() * 480.0);
        self.active = Active::Scroll(ScrollCursor::new(
            self.persona,
            scroll_px,
            now_us,
            self.seed ^ 0x5C20_0000_0000_0041,
            0,
        ));
        self.next_due_us = now_us;
    }

    fn enter_idle(&mut self, now_us: u64) {
        self.phase = TabPhase::Idle;
        self.active = Active::None;
        self.next_heartbeat_us = now_us + self.rng.next_range(4000, 12000) as u64 * 1000;
        self.next_due_us = self.next_heartbeat_us;
    }

    fn idle_tick(&mut self, now_us: u64, out: &mut SmallVec<[RawEvent; 32]>) {
        let t_ms = now_us / 1000;
        if let Some(flip) = visibility_flip(self.seed, self.ctx_id, t_ms) {
            out.push(RawEvent::new(0, 0, 0, input::FOCUS, flip.new_state));
        }
        if now_us >= self.next_heartbeat_us {
            let beat = mouse_heartbeat(self.seed, self.ctx_id, t_ms);
            if self.idle_bump_i >= self.idle_bump_path.len() {
                let dx = beat.path.last().map(|p| p.0).unwrap_or(0);
                let dy = beat.path.last().map(|p| p.1).unwrap_or(0);
                if dx == 0 && dy == 0 {
                    self.idle_bump_path = wind_mouse_path(
                        self.x,
                        self.y,
                        self.x + self.rng.next_range(0, 9) as f64 - 4.0,
                        self.y + self.rng.next_range(0, 9) as f64 - 4.0,
                        &mut self.rng,
                    );
                } else {
                    self.idle_bump_path = wind_mouse_path(self.x, self.y, self.x + dx as f64, self.y + dy as f64, &mut self.rng);
                }
                self.idle_bump_i = 0;
            }
            if let Some(&(px, py)) = self.idle_bump_path.get(self.idle_bump_i) {
                self.idle_bump_i += 1;
                self.x = px as f64;
                self.y = py as f64;
                out.push(RawEvent::new(
                    px.clamp(0, u16::MAX as i32) as u16,
                    py.clamp(0, u16::MAX as i32) as u16,
                    16,
                    IK_MOUSE_MOVE,
                    button::LEFT,
                ));
            }
            self.next_heartbeat_us = now_us + beat.next_interval_ms as u64 * 1000;
        }
        let drift = crate::input::trajectory::generate_idle_drift(self.x, self.y, self.seed as u32 ^ (t_ms as u32), 1);
        if let Some(p) = drift.first() {
            self.x = p.x as f64;
            self.y = p.y as f64;
            out.push(RawEvent::new(
                p.x.clamp(0, u16::MAX as i32) as u16,
                p.y.clamp(0, u16::MAX as i32) as u16,
                p.dt_ms.min(u16::MAX as u32) as u16,
                IK_MOUSE_MOVE,
                button::LEFT,
            ));
        }
        if self.rng.chance(self.persona.reading_pause_p * 0.4) {
            out.push(RawEvent::new(0, 3, 16, IK_SCROLL, 0));
        }
        self.next_due_us = self
            .next_heartbeat_us
            .min(now_us + 250_000 + self.rng.next_range(0, 750_000) as u64);
    }
}

pub struct TelemetryBatcher {
    cap: usize,
    interval_us: u64,
    len: usize,
    started_us: u64,
    batch_idx: u32,
}

impl TelemetryBatcher {
    pub fn new(cap: usize, interval_us: u64) -> Self {
        Self {
            cap,
            interval_us,
            len: 0,
            started_us: 0,
            batch_idx: 0,
        }
    }

    #[inline]
    pub fn begin(&mut self, now_us: u64) {
        self.len = 0;
        self.started_us = now_us;
    }

    #[inline]
    pub fn feed(&mut self, n: usize) {
        self.len += n;
    }

    #[inline]
    pub fn ready(&self, now_us: u64) -> bool {
        self.len >= self.cap || (self.len > 0 && now_us.saturating_sub(self.started_us) >= self.interval_us)
    }

    #[inline]
    pub fn take(&mut self) -> u32 {
        self.batch_idx = self.batch_idx.wrapping_add(1);
        self.batch_idx
    }
}

pub const BATCH_CAP: usize = 64;
pub const BATCH_INTERVAL_US: u64 = 2_000_000;
