mod b64;
mod collector;
mod dom;
mod nextdata;
mod pipeline;
mod probe;
mod scratch;
mod types;

pub use b64::{B64Error, decode_b64_field};
pub use collector::{floor_char_boundary, looks_like_token};
pub use dom::{DomTree, NodeId, ATTR_NAMES};
pub use pipeline::{Flow, Limits, PipeError, StreamPipeline};
pub use types::{ChallengeType, FieldData, FieldKind, Form, FormData, NextData, PageData};

use compact_str::CompactString;

/// Валидация CSS-селекторов перед постройкой rewriter'а (Extract-задачи).
pub fn validate_selectors(sels: &[(String, String)]) -> Result<(), String> {
    for (name, sel) in sels {
        sel.parse::<lol_html::Selector>().map_err(|e| {
            use std::fmt::Write;
            let mut s = String::with_capacity(name.len() + sel.len() + 4);
            let _ = write!(s, "{name} ({sel}): {e}");
            s
        })?;
    }
    Ok(())
}

use std::sync::atomic::{AtomicU64, Ordering};

/// Мониторинг версий челлендж-скриптов: xxh3 контента против сохранённого.
/// Первый check() для URL кладёт хэш и возвращает false, последующие —
/// true при изменении (выкатили новый билд wasm/js).
pub struct VersionMonitor {
    hashes: scc::HashMap<CompactString, AtomicU64>,
}

impl VersionMonitor {
    pub fn new() -> Self {
        Self { hashes: scc::HashMap::new() }
    }

    /// Возвращает true, если контент URL изменился с прошлого раза.
    pub fn check(&self, url: &str, content: &[u8]) -> bool {
        let new_hash = payload_gen::xxh3(content);
        let changed = self
            .hashes
            .read_sync(url, |_, v| v.load(Ordering::Relaxed) != new_hash)
            .unwrap_or(true);
        let _ = self.hashes.insert_sync(CompactString::new(url), AtomicU64::new(new_hash));
        changed
    }

    pub fn get_hash(&self, url: &str) -> Option<u64> {
        self.hashes.read_sync(url, |_, v| v.load(Ordering::Relaxed))
    }

    pub fn for_each_hash<F: FnMut(&str, u64)>(&self, mut f: F) {
        self.hashes.iter_sync(|url, v| {
            f(url.as_str(), v.load(Ordering::Relaxed));
            true
        });
    }
}

use bumpalo::Bump;
use std::cell::RefCell;

#[repr(align(64))]
pub struct ScratchArena {
    pub bump: Bump,
}

thread_local! {
    pub static SCRATCH: RefCell<ScratchArena> = RefCell::new(ScratchArena {
        bump: Bump::with_capacity(128 * 1024),
    });
}

/// Сброс арены + выполнение замыкания. 1 такт CPU, ноль malloc/free.
#[inline(always)]
pub fn run_task_scoped<F, R>(f: F) -> R
where
    F: FnOnce(&Bump) -> R,
{
    SCRATCH.with(|s| {
        let mut cell = s.borrow_mut();
        cell.bump.reset();
        f(&cell.bump)
    })
}

/// Нормализация chunk: fast path (нет \r) = zero-copy slice, slow path = bump Vec.
#[inline(always)]
pub fn normalize_stream<'a>(chunk: &'a [u8], bump: &'a Bump) -> &'a [u8] {
    if !chunk.contains(&b'\r') {
        return chunk;
    }
    let mut v = bumpalo::collections::Vec::with_capacity_in(chunk.len(), bump);
    for &b in chunk {
        if b != b'\r' {
            v.push(b);
        }
    }
    v.into_bump_slice()
}

/// Сборка payload-строки в арене — ноль аллокаций на куче.
/// Применение: подпись задачи в serve-диспетчере, key:id payload.
#[inline(always)]
pub fn build_payload<'a>(key: &str, id: u64, bump: &'a Bump) -> &'a str {
    use core::fmt::Write;
    let mut s = bumpalo::collections::String::new_in(bump);
    let _ = write!(s, "{key}:{id}");
    s.into_bump_str()
}