use crate::input::prng::SplitMix64Rng;

pub struct Perlin2D {
    perm: [u8; 512],
}

impl Perlin2D {
    pub fn from_seed(seed: u64) -> Self {
        let mut rng = SplitMix64Rng::new(seed);
        let mut p = [0u8; 256];
        for (i, slot) in p.iter_mut().enumerate() {
            *slot = i as u8;
        }
        for i in (1..256).rev() {
            let j = (rng.next_u64() % (i as u64 + 1)) as usize;
            p.swap(i, j);
        }
        let mut perm = [0u8; 512];
        perm[..256].copy_from_slice(&p);
        perm[256..].copy_from_slice(&p);
        Self { perm }
    }

    #[inline]
    pub fn noise(&self, x: f64, y: f64) -> f64 {
        let fx = if x < 0.0 { -((-x).floor()) } else { x.floor() };
        let fy = if y < 0.0 { -((-y).floor()) } else { y.floor() };
        let xi = fx as i32 & 255;
        let yi = fy as i32 & 255;
        let xf = x - fx;
        let yf = y - fy;
        let u = fade(xf);
        let v = fade(yf);
        let base = self.perm[xi as usize] as usize;
        let base1 = self.perm[(xi as usize + 1) & 0xFF] as usize;
        let x1 = lerp(
            grad(self.perm[(base + yi as usize) & 0x1FF], xf, yf),
            grad(self.perm[(base1 + yi as usize) & 0x1FF], xf - 1.0, yf),
            u,
        );
        let x2 = lerp(
            grad(self.perm[(base + yi as usize + 1) & 0x1FF], xf, yf - 1.0),
            grad(self.perm[(base1 + yi as usize + 1) & 0x1FF], xf - 1.0, yf - 1.0),
            u,
        );
        lerp(x1, x2, v) * 0.7071067811865476
    }

    pub fn fbm(&self, x: f64, y: f64, octaves: u32, persistence: f64, lacunarity: f64) -> f64 {
        let mut total = 0.0;
        let mut amplitude = 1.0;
        let mut frequency = 1.0;
        let mut max = 0.0;
        for _ in 0..octaves {
            total += self.noise(x * frequency, y * frequency) * amplitude;
            max += amplitude;
            amplitude *= persistence;
            frequency *= lacunarity;
        }
        total / max
    }
}

#[inline]
fn fade(t: f64) -> f64 {
    t * t * t * (t * (t * 6.0 - 15.0) + 10.0)
}

#[inline]
fn lerp(a: f64, b: f64, t: f64) -> f64 {
    a + t * (b - a)
}

#[inline]
fn grad(hash: u8, x: f64, y: f64) -> f64 {
    match hash % 12 {
        0 => x + y,
        1 => -x + y,
        2 => x - y,
        3 => -x - y,
        4 => x,
        5 => -x,
        6 => y,
        7 => -y,
        8 => x + y,
        9 => -x + y,
        10 => x - y,
        _ => -x - y,
    }
}
