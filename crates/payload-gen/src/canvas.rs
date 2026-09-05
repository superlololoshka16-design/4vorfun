use core_utils::xxh3;
use smallvec::SmallVec;
use std::io::Write;

const WEBGL_RANGES: [(f64, f64); 8] = [
    (1.0, 2048.0),
    (1.0, 1024.0),
    (3379.0, 16384.0),
    (0.0, 1.0),
    (8.0, 16.0),
    (64.0, 4096.0),
    (2.0, 16.0),
    (0.1, 1.0),
];

pub fn canvas_hash(seed: u64, vendor: &str, renderer: &str) -> [u8; 32] {
    let mut buf: SmallVec<[u8; 256]> = SmallVec::new();
    let _ = write!(&mut buf, "silo-canvas:0:{seed}:{vendor}:{renderer}");
    let h = xxh3::hash_seeded(0x5349_4C4F_4341_4E56, buf.as_slice());
    let mut pre = [0u8; 8];
    pre.copy_from_slice(&h.to_le_bytes());
    let mut out = [0u8; 32];
    let mut mix: SmallVec<[u8; 128]> = SmallVec::new();
    mix.extend_from_slice(&pre);
    mix.extend_from_slice(&h.to_le_bytes());
    mix.extend_from_slice(&h.rotate_left(17).to_le_bytes());
    mix.extend_from_slice(&h.rotate_left(43).to_le_bytes());
    crate::digest::sha256_into(mix.as_slice(), &mut out);
    out
}

pub fn webgl_param(seed: u64, slot: u8) -> f64 {
    let (lo, hi) = WEBGL_RANGES[(slot as usize) % WEBGL_RANGES.len()];
    let bits =
        xxh3::hash_seeded(0x5745_4247_4C53_4C4F, &seed.to_le_bytes()).rotate_left((slot as u32) * 7);
    let unit = (bits >> 11) as f64 / 9007199254740992.0;
    lo + unit * (hi - lo)
}
