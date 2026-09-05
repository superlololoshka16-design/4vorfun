use std::fs::File;
use std::path::Path;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum BundleError {
    #[error("polyfill unreadable: {0}")]
    Io(#[from] std::io::Error),
    #[error("polyfill not utf8")]
    Utf8,
}

pub struct Bundle {
    map: memmap2::Mmap,
}

impl Bundle {
    pub fn open(path: impl AsRef<Path>) -> Result<Self, BundleError> {
        let file = File::open(path)?;
        let map = unsafe { memmap2::Mmap::map(&file)? };
        if simdutf8::basic::from_utf8(&map).is_err() {
            return Err(BundleError::Utf8);
        }
        Ok(Self { map })
    }

    pub fn as_str(&self) -> &str {
        simdutf8::basic::from_utf8(&self.map).expect("validated at load")
    }

    pub fn len(&self) -> usize {
        self.map.len()
    }

    pub fn is_empty(&self) -> bool {
        self.map.is_empty()
    }
}
