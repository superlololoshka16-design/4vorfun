use crate::input::event::{RawEvent, button, input};
use crate::input::persona::Persona;
use crate::input::prng::SplitMix64Rng;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClickButton {
    Primary,
    Secondary,
    Auxiliary,
}

#[derive(Debug, Clone, Copy)]
pub struct ClickAuthenticityProfile {
    pub dblclick_max_ms: u32,
    pub min_press_ms: u32,
    pub max_press_ms: u32,
    pub min_release_gap_ms: u32,
    pub max_release_gap_ms: u32,
}

impl Default for ClickAuthenticityProfile {
    fn default() -> Self {
        Self {
            dblclick_max_ms: 500,
            min_press_ms: 60,
            max_press_ms: 160,
            min_release_gap_ms: 40,
            max_release_gap_ms: 220,
        }
    }
}

#[derive(Debug, Clone)]
pub struct ClickEvent {
    pub button: ClickButton,
    pub press_ms: u32,
    pub release_ms: u32,
    pub ts_unix_ms: u64,
}

impl ClickEvent {
    pub fn is_plausible(&self, profile: &ClickAuthenticityProfile) -> bool {
        self.press_ms >= profile.min_press_ms
            && self.press_ms <= profile.max_press_ms
            && self.release_ms >= profile.min_release_gap_ms
            && self.release_ms <= profile.max_release_gap_ms
    }

    pub fn forms_dblclick_with(&self, other: &ClickEvent) -> bool {
        if self.button != other.button {
            return false;
        }
        if other.ts_unix_ms < self.ts_unix_ms {
            return false;
        }
        let delta = other.ts_unix_ms.saturating_sub(self.ts_unix_ms);
        delta <= 500
    }
}

pub fn generate_click_timing(
    prng: &mut SplitMix64Rng,
    profile: &ClickAuthenticityProfile,
    button: ClickButton,
    ts_unix_ms: u64,
) -> ClickEvent {
    let press_ms = prng
        .lognormal_ms(95.0, 0.2)
        .clamp(profile.min_press_ms, profile.max_press_ms);
    let release_ms = prng
        .lognormal_ms(110.0, 0.3)
        .clamp(profile.min_release_gap_ms, profile.max_release_gap_ms);
    ClickEvent {
        button,
        press_ms,
        release_ms,
        ts_unix_ms,
    }
}

#[derive(Debug, Clone, Copy)]
pub struct ClickPlan {
    pub button: u8,
    pub is_double: bool,
    pub is_right: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Stage {
    Press,
    Hold,
    Release,
    Done,
}

pub struct ClickCursor {
    persona: Persona,
    x: u16,
    y: u16,
    stage: Stage,
    next_due_us: u64,
    plan: ClickPlan,
    second_pending: bool,
    rng: SplitMix64Rng,
}

impl ClickCursor {
    pub fn plan(persona: &Persona, seed: u64) -> ClickPlan {
        let mut rng = SplitMix64Rng::new(seed ^ 0xC1C_0000_0000_0001);
        let is_right = rng.chance(persona.right_click_p);
        let is_double = !is_right && rng.chance(persona.double_click_p);
        ClickPlan {
            button: if is_right { button::RIGHT } else { button::LEFT },
            is_double,
            is_right,
        }
    }

    pub fn new(persona: Persona, x: u16, y: u16, now_us: u64, seed: u64, trust: i32) -> Self {
        let _ = trust;
        let mut rng = SplitMix64Rng::new(seed ^ 0x0C1C_0000_0000_0055);
        let pre_press = rng.lognormal_ms(40.0, 0.3);
        Self {
            persona,
            x,
            y,
            stage: Stage::Press,
            next_due_us: now_us + pre_press as u64 * 1000,
            plan: Self::plan(&persona, seed),
            second_pending: false,
            rng: SplitMix64Rng::new(seed ^ 0x00D0_0000_0000_0042),
        }
    }

    #[inline]
    pub fn done(&self) -> bool {
        self.stage == Stage::Done
    }

    #[inline]
    pub fn next_due_us(&self) -> u64 {
        self.next_due_us
    }

    #[inline]
    pub fn plan_info(&self) -> ClickPlan {
        self.plan
    }

    pub fn step(&mut self, now_us: u64) -> Option<RawEvent> {
        if self.stage == Stage::Done || now_us < self.next_due_us {
            return None;
        }
        match self.stage {
            Stage::Press => {
                self.stage = Stage::Hold;
                let hold = self
                    .rng
                    .lognormal_ms(self.persona.dwell_median_ms, self.persona.dwell_sigma_ln);
                self.next_due_us = now_us + hold as u64 * 1000;
                Some(RawEvent::new(self.x, self.y, 0, input::PRESS, self.plan.button))
            }
            Stage::Hold => {
                self.stage = Stage::Release;
                let gap = self.rng.lognormal_ms(70.0, 0.35);
                self.next_due_us = now_us + gap as u64 * 1000;
                let dx = self.rng.next_range(0, 3) as i32 - 1;
                let dy = self.rng.next_range(0, 3) as i32 - 1;
                self.x = (self.x as i32 + dx).clamp(0, u16::MAX as i32) as u16;
                self.y = (self.y as i32 + dy).clamp(0, u16::MAX as i32) as u16;
                Some(RawEvent::new(self.x, self.y, 0, input::RELEASE, self.plan.button))
            }
            Stage::Release => {
                if self.plan.is_double && !self.second_pending {
                    self.second_pending = true;
                    self.stage = Stage::Press;
                    let dbl_gap = self.rng.lognormal_ms(110.0, 0.25);
                    self.next_due_us = now_us + dbl_gap as u64 * 1000;
                    return None;
                }
                self.stage = Stage::Done;
                None
            }
            Stage::Done => None,
        }
    }
}
