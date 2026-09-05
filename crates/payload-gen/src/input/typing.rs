use crate::input::event::{RawEvent, input};
use crate::input::persona::Persona;
use crate::input::prng::SplitMix64Rng;

const MIN_KEY_INTERVAL_MS: u32 = 45;
const MAX_KEY_INTERVAL_MS: u32 = 680;
const PAUSE_AFTER_WORD_MIN_MS: u32 = 100;
const PAUSE_AFTER_WORD_MAX_MS: u32 = 300;
const PAUSE_AFTER_SENTENCE_MIN_MS: u32 = 300;
const PAUSE_AFTER_SENTENCE_MAX_MS: u32 = 800;
const BACKSPACE_PROB: f64 = 0.05;
const CAPS_ERROR_PROB: f64 = 0.05;
const MIN_DELAY_MS: u32 = 50;
const SEED_LOG_NORMAL_A: u64 = 0x9E37_79B9_7F4A_7C15;
const SEED_LOG_NORMAL_B: u64 = 0x6A09_E667_F3BC_C908;
const SEED_PAUSE_WORD: u64 = 0xBB67_AE85_84CA_A73B;
const SEED_PAUSE_SENT: u64 = 0xA409_3822_299F_31D0;
const SEED_BACKSPACE: u64 = 0x510E_527F_ADE6_824D;
const SEED_CAPS_ERR: u64 = 0x84CF_E392_4F1A_6B7E;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TypingStep {
    pub key: char,
    pub delay_before_ms: u32,
    pub is_shift: bool,
    pub is_backspace: bool,
}

impl TypingStep {
    fn normal(key: char, delay_ms: u32, is_shift: bool) -> Self {
        Self {
            key,
            delay_before_ms: delay_ms,
            is_shift,
            is_backspace: false,
        }
    }

    fn backspace(delay_ms: u32) -> Self {
        Self {
            key: '\u{8}',
            delay_before_ms: delay_ms,
            is_shift: false,
            is_backspace: true,
        }
    }

    fn pause(delay_ms: u32) -> Self {
        Self {
            key: '\0',
            delay_before_ms: delay_ms,
            is_shift: false,
            is_backspace: false,
        }
    }
}

pub fn bigram_latency_ms(prev: char, cur: char) -> u32 {
    let p = prev.to_ascii_lowercase();
    let c = cur.to_ascii_lowercase();
    match (p, c) {
        ('t', 'h') => 80,
        ('h', 'e') => 85,
        ('i', 'n') => 90,
        ('e', 'r') => 88,
        ('a', 'n') => 92,
        ('r', 'e') => 90,
        ('o', 'n') => 95,
        ('a', 't') => 92,
        ('e', 'n') => 90,
        ('n', 'd') => 95,
        ('t', 'i') => 88,
        ('e', 's') => 90,
        ('o', 'r') => 92,
        ('t', 'e') => 88,
        ('o', 'f') => 95,
        ('e', 'd') => 90,
        ('i', 's') => 88,
        ('i', 't') => 90,
        ('a', 'l') => 92,
        ('a', 'r') => 90,
        ('s', 't') => 85,
        ('t', 'o') => 88,
        ('n', 't') => 90,
        ('n', 'g') => 92,
        ('s', 'e') => 90,
        ('h', 'a') => 92,
        ('a', 's') => 90,
        ('o', 'u') => 95,
        ('i', 'o') => 100,
        ('l', 'e') => 92,
        ('n', 'o') => 95,
        ('u', 's') => 95,
        ('c', 'o') => 100,
        ('m', 'e') => 92,
        ('d', 'e') => 95,
        ('h', 'i') => 95,
        ('r', 'i') => 95,
        ('r', 'o') => 95,
        ('i', 'c') => 100,
        ('n', 'e') => 92,
        ('e', 'a') => 95,
        ('r', 'a') => 95,
        ('c', 'e') => 100,
        ('q', 'x') | ('z', 'j') | ('k', 'x') | ('j', 'x') | ('q', 'z') | ('x', 'z')
        | ('j', 'q') | ('v', 'k') | ('b', 'x') | ('p', 'x') | ('z', 'z') | ('q', 'q')
        | ('x', 'x') => 200,
        _ => 120,
    }
}

pub fn log_normal_delay_ms(mean_ms: f64, std_ms: f64, seed: u64) -> u32 {
    let mut s = seed.wrapping_add(SEED_LOG_NORMAL_A);
    let u1 = {
        s = s.wrapping_add(0x9E3779B97F4A7C15);
        let mut z = s;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58476D1CE4E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D049BB133111EB);
        let v = (z ^ (z >> 31)) >> 12 | 0x3FF0_0000_0000_0000;
        let r01 = f64::from_bits(v) - 1.0;
        r01.max(1e-10)
    };
    let u2 = {
        s = s.wrapping_add(SEED_LOG_NORMAL_B);
        let mut z = s;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58476D1CE4E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D049BB133111EB);
        let v = (z ^ (z >> 31)) >> 12 | 0x3FF0_0000_0000_0000;
        f64::from_bits(v) - 1.0
    };
    let z = (-2.0 * u1.ln()).sqrt() * (core::f64::consts::TAU * u2).cos();
    let result = mean_ms + std_ms * z;
    if result < MIN_DELAY_MS as f64 {
        MIN_DELAY_MS
    } else {
        result as u32
    }
}

#[inline]
pub fn word_pause_ms(seed: u64) -> u32 {
    let mut rng = SplitMix64Rng::new(seed.wrapping_add(SEED_PAUSE_WORD));
    rng.lognormal_ms(170.0, 0.22).clamp(PAUSE_AFTER_WORD_MIN_MS, PAUSE_AFTER_WORD_MAX_MS)
}

#[inline]
pub fn sentence_pause_ms(seed: u64) -> u32 {
    let mut rng = SplitMix64Rng::new(seed.wrapping_add(SEED_PAUSE_SENT));
    rng.lognormal_ms(480.0, 0.25).clamp(PAUSE_AFTER_SENTENCE_MIN_MS, PAUSE_AFTER_SENTENCE_MAX_MS)
}

#[inline]
pub fn should_backspace(seed: u64) -> bool {
    SplitMix64Rng::new(seed.wrapping_add(SEED_BACKSPACE)).next_f64() < BACKSPACE_PROB
}

#[inline]
pub fn should_capitalize_error(seed: u64) -> bool {
    SplitMix64Rng::new(seed.wrapping_add(SEED_CAPS_ERR)).next_f64() < CAPS_ERROR_PROB
}

#[inline]
fn is_shifted_char(c: char) -> bool {
    c.is_ascii_uppercase()
        || matches!(
            c,
            '!' | '@' | '#' | '$' | '%' | '^' | '&' | '*' | '(' | ')' | '_' | '+' | '{' | '}'
                | '|' | ':' | '"' | '<' | '>' | '?' | '~'
        )
}

pub fn generate_typing_timeline(text: &str, seed: u64) -> Vec<TypingStep> {
    let chars: Vec<char> = text.chars().collect();
    if chars.is_empty() {
        return Vec::default();
    }
    let mut steps: Vec<TypingStep> = Vec::with_capacity(chars.len() + 4);
    let mut salt = seed;
    let mut prev = ' ';
    let mut idx: u64 = 0;
    for &c in &chars {
        let step_seed = seed.wrapping_add(idx.wrapping_mul(0x9E3779B97F4A7C15));
        let base = bigram_latency_ms(prev, c) as f64;
        let mut delay = log_normal_delay_ms(base, 40.0, step_seed);
        if delay < MIN_KEY_INTERVAL_MS {
            delay = MIN_KEY_INTERVAL_MS;
        }
        if delay > MAX_KEY_INTERVAL_MS {
            delay = MAX_KEY_INTERVAL_MS;
        }
        let is_shift = is_shifted_char(c);
        steps.push(TypingStep::normal(c, delay, is_shift));
        if should_backspace(step_seed) && idx > 0 {
            let bs_seed = step_seed.wrapping_add(0xDEAD_BEEF_CAFE_BABE);
            let bs_delay = log_normal_delay_ms(180.0, 50.0, bs_seed);
            steps.push(TypingStep::backspace(bs_delay));
            let retype_seed = bs_seed.wrapping_add(0xBAAD_F00D_1234_5678);
            let retype_delay = log_normal_delay_ms(base, 40.0, retype_seed);
            steps.push(TypingStep::normal(c, retype_delay, is_shift));
        }
        if c == ' ' {
            salt = salt.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
            let pause = word_pause_ms(salt);
            steps.push(TypingStep::pause(pause));
        }
        if matches!(c, '.' | '!' | '?') {
            salt = salt.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
            let pause = sentence_pause_ms(salt);
            steps.push(TypingStep::pause(pause));
        }
        prev = c;
        idx = idx.wrapping_add(1);
    }
    steps
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
    pending_backspace: bool,
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
            pending_backspace: false,
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
        if self.pending_backspace {
            self.pending_backspace = false;
            self.idx = self.idx.saturating_sub(1);
            self.prev = if self.idx > 0 { self.chars[self.idx - 1] } else { ' ' };
            self.next_due_us = now_us + self.rng.lognormal_ms(150.0, 0.25) as u64 * 1000;
            return Some(RawEvent::new(8, 0, 0, input::KEY_DOWN, 0));
        }
        let Some(&c) = self.chars.get(self.idx) else {
            self.finished = true;
            return None;
        };
        let base = bigram_latency_ms(self.prev, c) as f64 * wpm_scale;
        let dt = self.rng.lognormal_ms(base, self.persona.typing_sigma_ln);
        self.next_due_us = now_us + dt as u64 * 1000;
        if self.idx > 0 && self.rng.chance(self.persona.backspace_p) {
            self.pending_backspace = true;
        }
        self.idx += 1;
        self.prev = c;
        let arg = u8::from(is_shifted_char(c));
        self.burst_left = self.burst_left.saturating_sub(1);
        if self.burst_left == 0 {
            let pause = self.rng.lognormal_ms(self.persona.burst_pause_median_ms, 0.35);
            self.next_due_us += pause as u64 * 1000;
            self.burst_left = self.persona.burst_len;
        }
        Some(RawEvent::new(c as u16, 0, 0, input::KEY_DOWN, arg))
    }
}
