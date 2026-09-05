//! Фасад внешних либ: версии и фичи живут в корневом `[workspace.dependencies]`,
//! имя либы — только здесь. Смена парсера JSON или хеш-крейта — одна строка
//! в этом файле, ни один `use` в движке не ломается.

pub use sonic_rs as json;
pub use base64_simd as base64;
pub use simdutf8 as utf8;

pub mod xxh3 {
    pub use twox_hash::XxHash3_64;

    #[inline(always)]
    pub fn hash(data: &[u8]) -> u64 {
        XxHash3_64::oneshot(data)
    }

    #[inline(always)]
    pub fn hash_seeded(seed: u64, data: &[u8]) -> u64 {
        XxHash3_64::oneshot_with_seed(seed, data)
    }
}
