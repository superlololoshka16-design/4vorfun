//! Скретч стрим-парсера: та же нить, что и rewriter — ноль каналов,
//! ноль аллокаций на событие и на строку.
//!
//! Архитектура (по README: «строки — в арену со смещениями, события — Copy»):
//! - хендлеры lol_html кладут строки в байтовую арену и получают `Span`
//!   (u32-смещение + u32-длина) — перекладки String/CompactString нет;
//! - события `Ev` (спаны и u16/u32) летят в плоский `Vec<Ev>` —
//!   push амортизированный, heap не трогается;
//! - атрибуты элементов — отдельный плоский `Vec<AttrEv>`, событие несёт
//!   `(attr_start, attr_count)` — SoA вместо SmallVec в каждом событии;
//! - `drain_into` вызывается в конце каждого `push()` синхронно: события
//!   не переживают await, интерливинг задач на нити безопасен;
//! - после дренча арена и атрибуты очищаются, capacity сохраняется —
//!   следующий документ идёт по той же памяти без malloc/free.

use std::cell::RefCell;

/// Спан строки в арене скретча.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Span {
    pub off: u32,
    pub len: u32,
}

pub(crate) const NIL_SPAN: Span = Span { off: 0, len: 0 };

impl Span {
    /// Резолв спана в &str. В арене лежит только валидный UTF-8 —
    /// всё, что туда попало, пришло из `&str` (граница lol_html).
    #[inline(always)]
    pub fn get<'a>(self, pool: &'a [u8]) -> &'a str {
        let s = self.off as usize;
        let e = s + self.len as usize;
        debug_assert!(e <= pool.len(), "span out of scratch pool");
        unsafe { core::str::from_utf8_unchecked(pool.get_unchecked(s..e)) }
    }
}

/// Атрибут элемента: интерн имени + спан значения. Copy, 12 байт.
#[derive(Debug, Clone, Copy)]
pub(crate) struct AttrEv {
    pub name: u16,
    pub name_dyn: Option<Span>,
    pub value: Span,
}

/// События парсера: только спаны и числа — ни одной строки, ни одного клона.
pub(crate) enum Ev {
    /// Открытие `<script>`: спан src (None = inline).
    Script { src: Option<Span>, is_next_data: bool },
    /// Чанк текста скрипта; `last` — конец текстового узла.
    ScriptText { span: Span, last: bool },
    TitleOpen,
    TitleText { span: Span, last: bool },
    MetaDescription { span: Span },
    Form { action: Option<Span>, method: Span },
    Field {
        tag: Span,
        name: Span,
        value: Option<Span>,
        kind: Span,
        hidden: bool,
        id: Option<Span>,
    },
    /// Текст по CSS-селектору: ключ — индекс в таблице ключей коллектора.
    Extract { key: u32, span: Span },
    /// Открытие элемента: тег (интерн) + диапазон атрибутов в плоском векторе.
    DomOpen { tag: u16, tag_dyn: Option<Span>, attr_start: u32, attr_count: u8 },
    /// Текстовый чанк вне script/style (фильтруется коллектором).
    DomText { span: Span },
    /// Закрытие элемента.
    DomClose { tag: u16, tag_dyn: Option<Span> },
    /// Маркер челленджа в src скрипта: спан src + индекс паттерна.
    ChallengeDetected { marker: Span, ct: u8 },
}

pub(crate) struct EvScratch {
    pool: Vec<u8>,
    events: Vec<Ev>,
    attrs: Vec<AttrEv>,
}

impl EvScratch {
    fn new() -> Self {
        Self {
            pool: Vec::with_capacity(64 * 1024),
            events: Vec::with_capacity(512),
            attrs: Vec::with_capacity(512),
        }
    }
}

thread_local! {
    static EV: RefCell<EvScratch> = RefCell::new(EvScratch::new());
}

/// Положить строку в арену — одна memcpy, ноль промежуточных аллокаций.
#[inline(always)]
pub(crate) fn push_str(s: &str) -> Span {
    EV.with(|c| {
        let mut sc = c.borrow_mut();
        let off = sc.pool.len() as u32;
        sc.pool.extend_from_slice(s.as_bytes());
        Span { off, len: s.len() as u32 }
    })
}

/// Отметка позиции в векторе атрибутов (перед серией push_attr).
#[inline(always)]
pub(crate) fn attr_mark() -> u32 {
    EV.with(|c| c.borrow_mut().attrs.len() as u32)
}

/// Положить атрибут в плоский вектор.
#[inline(always)]
pub(crate) fn push_attr(a: AttrEv) {
    EV.with(|c| c.borrow_mut().attrs.push(a));
}

/// Отправить событие — перемещение, без аллокаций.
#[inline(always)]
pub(crate) fn emit(ev: Ev) {
    EV.with(|c| c.borrow_mut().events.push(ev));
}

/// Дренч всех событий в коллектор. Вызывается строго в конце `push()`/
/// `finish()` — синхронно, до любого await. Арена и атрибуты очищаются,
/// capacity остаётся: следующий write() пишет в ту же память.
pub(crate) fn drain_into(collector: &mut crate::collector::Collector) {
    EV.with(|c| {
        let mut sc = c.borrow_mut();
        let EvScratch { pool, events, attrs } = &mut *sc;
        for ev in events.drain(..) {
            collector.apply(pool, attrs, ev);
        }
        attrs.clear();
        pool.clear();
    });
}
