
use std::cell::RefCell;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Span {
    pub off: u32,
    pub len: u32,
}

pub(crate) const NIL_SPAN: Span = Span { off: 0, len: 0 };

impl Span {
    #[inline(always)]
    pub fn get<'a>(self, pool: &'a [u8]) -> &'a str {
        let s = self.off as usize;
        let e = s + self.len as usize;
        debug_assert!(e <= pool.len(), "span out of scratch pool");
        unsafe { core::str::from_utf8_unchecked(pool.get_unchecked(s..e)) }
    }
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct AttrEv {
    pub name: u16,
    pub name_dyn: Option<Span>,
    pub value: Span,
}

pub(crate) enum Ev {
    Script { src: Option<Span>, is_next_data: bool },
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
    Extract { key: u32, span: Span },
    DomOpen { tag: u16, tag_dyn: Option<Span>, attr_start: u32, attr_count: u8 },
    DomText { span: Span },
    DomClose { tag: u16, tag_dyn: Option<Span> },
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

#[inline(always)]
pub(crate) fn push_str(s: &str) -> Span {
    EV.with(|c| {
        let mut sc = c.borrow_mut();
        let off = sc.pool.len() as u32;
        sc.pool.extend_from_slice(s.as_bytes());
        Span { off, len: s.len() as u32 }
    })
}

#[inline(always)]
pub(crate) fn attr_mark() -> u32 {
    EV.with(|c| c.borrow_mut().attrs.len() as u32)
}

#[inline(always)]
pub(crate) fn push_attr(a: AttrEv) {
    EV.with(|c| c.borrow_mut().attrs.push(a));
}

#[inline(always)]
pub(crate) fn emit(ev: Ev) {
    EV.with(|c| c.borrow_mut().events.push(ev));
}

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
