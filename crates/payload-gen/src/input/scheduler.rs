use crate::click::ClickCursor;
use crate::input::event::RawEvent;
use crate::motion::MotionCursor;
use crate::scroll::ScrollCursor;
use crate::typing::TypingCursor;
use std::cmp::Reverse;
use std::collections::BinaryHeap;
use std::sync::Arc;
use std::sync::atomic::{AtomicI32, AtomicU32, Ordering};

pub struct Calibration {
    strictness: u8,
    trust: AtomicI32,
    passes: AtomicU32,
    fails: AtomicU32,
}

impl Calibration {
    pub fn new(strictness: u8) -> Arc<Self> {
        Arc::new(Self {
            strictness,
            trust: AtomicI32::new(0),
            passes: AtomicU32::new(0),
            fails: AtomicU32::new(0),
        })
    }

    pub fn record(&self, passed: bool) {
        let w = 1 + self.strictness as i32;
        if passed {
            self.passes.fetch_add(1, Ordering::Relaxed);
            self.trust.fetch_add(w, Ordering::AcqRel);
        } else {
            self.fails.fetch_add(1, Ordering::Relaxed);
            self.trust.fetch_sub(w, Ordering::AcqRel);
        }
    }

    #[inline]
    pub fn trust(&self) -> i32 {
        self.trust.load(Ordering::Acquire)
    }

    #[inline]
    pub fn stats(&self) -> (u32, u32) {
        (
            self.passes.load(Ordering::Relaxed),
            self.fails.load(Ordering::Relaxed),
        )
    }
}

pub enum TabInput {
    Move(MotionCursor),
    Scroll(ScrollCursor),
    Type(TypingCursor),
    Click(ClickCursor),
}

impl TabInput {
    #[inline]
    fn next_due_us(&self) -> u64 {
        match self {
            TabInput::Move(c) => c.next_due_us(),
            TabInput::Scroll(c) => c.next_due_us(),
            TabInput::Type(c) => c.next_due_us(),
            TabInput::Click(c) => c.next_due_us(),
        }
    }

    #[inline]
    fn done(&self) -> bool {
        match self {
            TabInput::Move(c) => c.done(),
            TabInput::Scroll(c) => c.done(),
            TabInput::Type(c) => c.done(),
            TabInput::Click(c) => c.done(),
        }
    }

    fn step(&mut self, now_us: u64) -> Option<RawEvent> {
        match self {
            TabInput::Move(c) => c.step(now_us),
            TabInput::Scroll(c) => c.step(now_us),
            TabInput::Type(c) => c.step(now_us),
            TabInput::Click(c) => c.step(now_us),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TabId(pub u32);

struct TabSlot {
    site: u32,
    weight: u32,
    input: Option<TabInput>,
}

pub struct InputHub {
    tabs: Vec<TabSlot>,
    free: Vec<u32>,
    heap: BinaryHeap<Reverse<(u64, u32, u32)>>,
    sites: Vec<Arc<Calibration>>,
}

impl InputHub {
    pub fn new() -> Self {
        Self {
            tabs: Vec::new(),
            free: Vec::new(),
            heap: BinaryHeap::new(),
            sites: Vec::new(),
        }
    }

    pub fn register_site(&mut self, strictness: u8) -> u32 {
        self.sites.push(Calibration::new(strictness));
        self.sites.len() as u32 - 1
    }

    pub fn calibration(&self, site: u32) -> Option<&Arc<Calibration>> {
        self.sites.get(site as usize)
    }

    pub fn open_tab(&mut self, site: u32, weight: u32) -> TabId {
        let slot = TabSlot {
            site,
            weight,
            input: None,
        };
        let id = if let Some(idx) = self.free.pop() {
            self.tabs[idx as usize] = slot;
            idx
        } else {
            self.tabs.push(slot);
            (self.tabs.len() - 1) as u32
        };
        TabId(id)
    }

    pub fn close_tab(&mut self, tab: TabId) {
        let i = tab.0 as usize;
        if i < self.tabs.len() && self.tabs[i].site != u32::MAX {
            self.tabs[i].site = u32::MAX;
            self.tabs[i].input = None;
            self.free.push(tab.0);
        }
    }

    pub fn set_input(&mut self, tab: TabId, input: TabInput) {
        let i = tab.0 as usize;
        if i >= self.tabs.len() || self.tabs[i].site == u32::MAX {
            return;
        }
        let due = input.next_due_us();
        let rank = self.tab_rank(i);
        self.tabs[i].input = Some(input);
        self.heap.push(Reverse((due, rank, tab.0)));
    }

    #[inline]
    fn tab_rank(&self, i: usize) -> u32 {
        let slot = &self.tabs[i];
        let trust = self
            .sites
            .get(slot.site as usize)
            .map(|c| c.trust())
            .unwrap_or(0);
        let trust_bonus = (trust.max(0) as u32).min(64);
        u32::MAX - (slot.weight.saturating_add(trust_bonus)).min(u32::MAX - 1)
    }

    pub fn tick(&mut self, now_us: u64, out: &mut smallvec::SmallVec<[(TabId, RawEvent); 32]>) {
        out.clear();
        while let Some(&Reverse((due, _, tab_idx))) = self.heap.peek() {
            if due > now_us {
                break;
            }
            self.heap.pop();
            let i = tab_idx as usize;
            if i >= self.tabs.len() || self.tabs[i].site == u32::MAX {
                continue;
            }
            let Some(input) = self.tabs[i].input.as_mut() else {
                continue;
            };
            match input.step(now_us) {
                Some(ev) => {
                    out.push((TabId(tab_idx), ev));
                    if !input.done() {
                        let nd = input.next_due_us();
                        let rank = self.tab_rank(i);
                        self.heap.push(Reverse((nd, rank, tab_idx)));
                    } else {
                        self.tabs[i].input = None;
                    }
                }
                None => {
                    if input.done() {
                        self.tabs[i].input = None;
                    } else {
                        let nd = input.next_due_us();
                        let rank = self.tab_rank(i);
                        self.heap.push(Reverse((nd, rank, tab_idx)));
                    }
                }
            }
        }
    }

    pub fn live_tabs(&self) -> usize {
        self.tabs.iter().filter(|t| t.site != u32::MAX).count()
    }
}

impl Default for InputHub {
    fn default() -> Self {
        Self::new()
    }
}
