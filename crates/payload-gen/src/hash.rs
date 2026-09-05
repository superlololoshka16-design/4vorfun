use twox_hash::XxHash3_64;

pub fn xxh3(data: &[u8]) -> u64 {
    XxHash3_64::oneshot(data)
}

pub fn xxh3_seeded(seed: u64, data: &[u8]) -> u64 {
    XxHash3_64::oneshot_with_seed(seed, data)
}
