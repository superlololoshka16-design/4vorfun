#[derive(Debug, Clone, Copy)]
pub struct SplitMix64Rng {
    state: u64,
}

impl SplitMix64Rng {
    #[inline]
    pub const fn new(seed: u64) -> Self {
        Self { state: seed }
    }

    #[inline]
    pub const fn stepped(mut self, n: u64) -> Self {
        self.state = self.state.wrapping_add(n.wrapping_mul(0x9E3779B97F4A7C15));
        self
    }

    #[inline]
    pub fn next_u64(&mut self) -> u64 {
        self.state = self.state.wrapping_add(0x9E3779B97F4A7C15);
        let mut z = self.state;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58476D1CE4E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D049BB133111EB);
        z ^ (z >> 31)
    }

    #[inline]
    pub fn next_f64(&mut self) -> f64 {
        ((self.next_u64() >> 11) as f64) * (1.0 / 9007199254740992.0)
    }

    #[inline]
    pub fn next_range(&mut self, lo: u32, hi: u32) -> u32 {
        if hi <= lo {
            return lo;
        }
        lo + (self.next_u64() % (hi - lo) as u64) as u32
    }

    #[inline]
    pub fn chance(&mut self, p: f64) -> bool {
        self.next_f64() < p
    }

    #[inline]
    pub fn gauss(&mut self) -> f64 {
        let u1 = self.next_f64().max(1e-12);
        let u2 = self.next_f64();
        (-2.0 * u1.ln()).sqrt() * (core::f64::consts::TAU * u2).cos()
    }

    #[inline]
    pub fn lognormal_ms(&mut self, median_ms: f64, sigma_ln: f64) -> u32 {
        let v = (median_ms.ln() + self.gauss() * sigma_ln).exp();
        v.clamp(1.0, 65_535.0) as u32
    }
}
