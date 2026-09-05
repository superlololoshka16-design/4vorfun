pub mod input {
    pub const MOVE: u8 = 0;
    pub const PRESS: u8 = 1;
    pub const RELEASE: u8 = 2;
    pub const WHEEL: u8 = 3;
    pub const KEY_DOWN: u8 = 4;
    pub const KEY_UP: u8 = 5;
    pub const FOCUS: u8 = 6;
    pub const BLUR: u8 = 7;
}

pub mod button {
    pub const LEFT: u8 = 0;
    pub const MIDDLE: u8 = 1;
    pub const RIGHT: u8 = 2;
}

#[repr(C, packed)]
#[derive(Debug, Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
pub struct RawEvent {
    pub x: u16,
    pub y: u16,
    pub dt_ms: u16,
    pub kind: u8,
    pub arg: u8,
}

impl RawEvent {
    #[inline]
    pub const fn new(x: u16, y: u16, dt_ms: u16, kind: u8, arg: u8) -> Self {
        Self { x, y, dt_ms, kind, arg }
    }
}

pub const RAW_EVENT_LEN: usize = core::mem::size_of::<RawEvent>();

#[inline]
pub fn events_bytes(events: &[RawEvent]) -> &[u8] {
    bytemuck::cast_slice(events)
}
