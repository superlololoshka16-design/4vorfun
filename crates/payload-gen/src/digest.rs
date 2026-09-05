use md5::Digest as _;
use smallvec::SmallVec;

pub struct PowSolution {
    pub nonce: u64,
    pub digest: [u8; 32],
}

pub fn sha256_into(data: &[u8], out: &mut [u8; 32]) {
    *out = sha2::Sha256::digest(data).into();
}

pub fn sha256_hex_into(data: &[u8], out: &mut [u8; 64]) {
    let d = sha2::Sha256::digest(data);
    hex::encode_to_slice(d, out).expect("64-byte hex sink");
}

pub fn md5_hex_into(data: &[u8], out: &mut [u8; 32]) {
    let d: [u8; 16] = md5::Md5::digest(data).into();
    hex::encode_to_slice(d, out).expect("32-byte hex sink");
}

fn leading_zero_bits(digest: &[u8; 32]) -> u32 {
    let mut total = 0u32;
    for &b in digest {
        if b == 0 {
            total += 8;
        } else {
            total += b.leading_zeros();
            break;
        }
    }
    total
}

fn pow_hash(prefix: &[u8], nonce: u64, scratch: &mut SmallVec<[u8; 128]>) -> [u8; 32] {
    scratch.clear();
    scratch.extend_from_slice(prefix);
    scratch.extend_from_slice(&nonce.to_le_bytes());
    let mut d = [0u8; 32];
    sha256_into(scratch, &mut d);
    d
}

pub fn pow_search(prefix: &[u8], bits: u32, max_iters: u64) -> Option<PowSolution> {
    if bits > 63 {
        return None;
    }
    let mut scratch: SmallVec<[u8; 128]> = SmallVec::new();
    let mut nonce = 0u64;
    while nonce < max_iters {
        let d = pow_hash(prefix, nonce, &mut scratch);
        if leading_zero_bits(&d) >= bits {
            return Some(PowSolution { nonce, digest: d });
        }
        nonce = nonce.wrapping_add(1);
        if nonce == 0 {
            break;
        }
    }
    None
}
