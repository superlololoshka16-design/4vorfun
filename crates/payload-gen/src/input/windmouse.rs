use crate::input::prng::SplitMix64Rng;

#[derive(Debug, Clone, Copy)]
pub struct WindMouseParams {
    pub gravity: f64,
    pub wind: f64,
    pub max_step: f64,
    pub damped_distance: f64,
}

impl Default for WindMouseParams {
    fn default() -> Self {
        Self {
            gravity: 9.0,
            wind: 3.0,
            max_step: 15.0,
            damped_distance: 12.0,
        }
    }
}

pub fn wind_mouse(
    start_x: f64,
    start_y: f64,
    dest_x: f64,
    dest_y: f64,
    params: WindMouseParams,
    rng: &mut impl FnMut() -> f64,
    out: &mut Vec<(i32, i32)>,
) {
    let sqrt3 = 3.0_f64.sqrt();
    let sqrt5 = 5.0_f64.sqrt();
    let mut current_x = start_x as i32;
    let mut current_y = start_y as i32;
    let mut x = start_x;
    let mut y = start_y;
    let mut vx = 0.0;
    let mut vy = 0.0;
    let mut wind_x = 0.0;
    let mut wind_y = 0.0;
    let mut max_step = params.max_step;
    let mut dist = ((dest_x - x).powi(2) + (dest_y - y).powi(2)).sqrt();
    while dist >= 1.0 {
        let wind_mag = params.wind.min(dist);
        if dist >= params.damped_distance {
            wind_x = wind_x / sqrt3 + (2.0 * rng() - 1.0) * wind_mag / sqrt5;
            wind_y = wind_y / sqrt3 + (2.0 * rng() - 1.0) * wind_mag / sqrt5;
        } else {
            wind_x /= sqrt3;
            wind_y /= sqrt3;
            if max_step < 3.0 {
                max_step = rng() * 3.0 + 3.0;
            } else {
                max_step /= sqrt5;
            }
        }
        vx += wind_x + params.gravity * (dest_x - x) / dist;
        vy += wind_y + params.gravity * (dest_y - y) / dist;
        let v_mag = (vx * vx + vy * vy).sqrt();
        if v_mag > max_step {
            let clip = max_step / 2.0 + rng() * max_step / 2.0;
            vx = (vx / v_mag) * clip;
            vy = (vy / v_mag) * clip;
        }
        x += vx;
        y += vy;
        let new_x = x.round() as i32;
        let new_y = y.round() as i32;
        if new_x != current_x || new_y != current_y {
            current_x = new_x;
            current_y = new_y;
            out.push((new_x, new_y));
        }
        dist = ((dest_x - x).powi(2) + (dest_y - y).powi(2)).sqrt();
    }
    if current_x != dest_x as i32 || current_y != dest_y as i32 {
        out.push((dest_x as i32, dest_y as i32));
    }
}

pub struct WindMouseStream {
    params: WindMouseParams,
    x: f64,
    y: f64,
    vx: f64,
    vy: f64,
    wind_x: f64,
    wind_y: f64,
    max_step: f64,
    dest_x: f64,
    dest_y: f64,
    rng: SplitMix64Rng,
    done: bool,
}

impl WindMouseStream {
    pub fn new(params: WindMouseParams, from: (f64, f64), to: (f64, f64), seed: u64) -> Self {
        Self {
            params,
            x: from.0,
            y: from.1,
            vx: 0.0,
            vy: 0.0,
            wind_x: 0.0,
            wind_y: 0.0,
            max_step: params.max_step,
            dest_x: to.0,
            dest_y: to.1,
            rng: SplitMix64Rng::new(seed),
            done: false,
        }
    }

    #[inline]
    pub fn is_done(&self) -> bool {
        self.done
    }

    pub fn next_tick(&mut self) -> Option<(i32, i32)> {
        if self.done {
            return None;
        }
        let mut r = self.rng;
        let mut noise = || r.next_f64();
        let sqrt3 = 3.0_f64.sqrt();
        let sqrt5 = 5.0_f64.sqrt();
        let dist = ((self.dest_x - self.x).powi(2) + (self.dest_y - self.y).powi(2)).sqrt();
        if dist < 1.0 {
            self.done = true;
            return Some((self.dest_x as i32, self.dest_y as i32));
        }
        let wind_mag = self.params.wind.min(dist);
        if dist >= self.params.damped_distance {
            self.wind_x = self.wind_x / sqrt3 + (2.0 * noise() - 1.0) * wind_mag / sqrt5;
            self.wind_y = self.wind_y / sqrt3 + (2.0 * noise() - 1.0) * wind_mag / sqrt5;
        } else {
            self.wind_x /= sqrt3;
            self.wind_y /= sqrt3;
            if self.max_step < 3.0 {
                self.max_step = noise() * 3.0 + 3.0;
            } else {
                self.max_step /= sqrt5;
            }
        }
        self.vx += self.wind_x + self.params.gravity * (self.dest_x - self.x) / dist;
        self.vy += self.wind_y + self.params.gravity * (self.dest_y - self.y) / dist;
        let v_mag = (self.vx * self.vx + self.vy * self.vy).sqrt();
        if v_mag > self.max_step {
            let clip = self.max_step * (0.5 + noise() * 0.5);
            self.vx = (self.vx / v_mag) * clip;
            self.vy = (self.vy / v_mag) * clip;
        }
        self.x += self.vx;
        self.y += self.vy;
        Some((self.x.round() as i32, self.y.round() as i32))
    }
}
