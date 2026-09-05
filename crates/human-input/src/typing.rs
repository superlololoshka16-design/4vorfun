use crate::event::{RawEvent, input};
use crate::persona::Persona;
use crate::prng::SplitMix64Rng;

#[inline]
fn bigram_ms(prev: char, cur: char) -> f64 {
    let p = prev.to_ascii_lowercase();
    let c = cur.to_ascii_lowercase();
    let same_hand_penalty =
        f64::from(matches!(
            (p, c),
            ('q', 'w') | ('w', 'q') | ('a', 's') | ('s', 'a') | ('z', 'x') | ('x', 'z')
        )) * 8.0;
    let home_row = |ch: char| {
        f64::from(matches!(
            ch,
            'a' | 's' | 'd' | 'f' | 'j' | 'k' | 'l' | ';'
        ))
    };
    let travel = match (p, c) {
        ('e', 'd') | ('d', 'e') | ('r', 'f') | ('f', 'r') => 0.85,
        ('u', 'j') | ('j', 'u') | ('i', 'k') | ('k', 'i') => 0.85,
        ('y', 'h') | ('h', 'y') | ('t', 'g') | ('g', 't') => 0.9,
        ('q', 'p') | ('p', 'q') | ('z', 'm') | ('m', 'z') => 1.6,
        _ => 1.0,
    };
    80.0 * (travel + same_hand_penalty) * (2.0 - home_row(p) * 0.5 - home_row(c) * 0.5)
}

#[inline]
fn is_shifted(c: char) -> bool {
    c.is_ascii_uppercase()
        || matches!(
            c,
            '!' | '@' | '#' | '$' | '%' | '^' | '&' | '*' | '(' | ')' | '_' | '+' | '{' | '}'
                | '|' | ':' | '"' | '<' | '>' | '?' | '~'
        )
}

pub struct TypingCursor {
    persona: Persona,
    chars: Vec<char>,
    idx: usize,
    prev: char,
    burst_left: u32,
    next_due_us: u64,
    finished: bool,
    rng: SplitMix64Rng,
    pending_backspace: Option<char>,
}

impl TypingCursor {
    pub fn new(persona: Persona, text: &str, now_us: u64, seed: u64, trust: i32) -> Self {
        let mut rng = SplitMix64Rng::new(seed ^ 0x7966_6557_4E45_4759);
        let wpm_scale = 60.0 / persona.wpm.max(10.0);
        let _ = trust;
        Self {
            persona,
            chars: text.chars().collect(),
            idx: 0,
            prev: ' ',
            burst_left: persona.burst_len,
            next_due_us: now_us + rng.lognormal_ms(240.0 * wpm_scale, 0.35) as u64 * 1000,
            finished: false,
            rng,
            pending_backspace: None,
        }
    }

    #[inline]
    pub fn done(&self) -> bool {
        self.finished
    }

    #[inline]
    pub fn next_due_us(&self) -> u64 {
        self.next_due_us
    }

    pub fn step(&mut self, now_us: u64) -> Option<RawEvent> {
        if self.finished || now_us < self.next_due_us {
            return None;
        }
        let wpm_scale = 60.0 / self.persona.wpm.max(10.0);
        if let Some(c) = self.pending_backspace.take() {
            self.next_due_us = now_us + self.rng.lognormal_ms(150.0, 0.25) as u64 * 1000;
            self.idx -= 1;
            self.prev = if self.idx > 0 { self.chars[self.idx - 1] } else { ' ' };
            let _ = c;
            return Some(RawEvent::new(8, 0, 0, input::KEY_DOWN, 0));
        }
        let Some(&c) = self.chars.get(self.idx) else {
            self.finished = true;
            return None;
        };
        let base = bigram_ms(self.prev, c) * wpm_scale;
        let dt = self.rng.lognormal_ms(base, self.persona.typing_sigma_ln);
        self.next_due_us = now_us + dt as u64 * 1000;
        if self.idx > 0 && self.rng.chance(self.persona.backspace_p) {
            self.pending_backspace = Some(c);
        }
        self.idx += 1;
        self.prev = c;
        let arg = u8::from(is_shifted(c));
        self.burst_left = self.burst_left.saturating_sub(1);
        if self.burst_left == 0 {
            let pause = self.rng.lognormal_ms(self.persona.burst_pause_median_ms, 0.35);
            self.next_due_us += pause as u64 * 1000;
            self.burst_left = self.persona.burst_len;
        }
        Some(RawEvent::new(c as u16, 0, 0, input::KEY_DOWN, arg))
    }
}
