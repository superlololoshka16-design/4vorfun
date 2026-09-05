use crate::input::prng::SplitMix64Rng;

const HOVER_PROB: f64 = 0.95;
const HOVER_MEDIAN_MS: f64 = 105.0;
const PRESS_MEDIAN_MS: f64 = 95.0;
const DOUBLE_CLICK_PROB: f64 = 0.05;
const DOUBLE_CLICK_GAP_MEDIAN_MS: f64 = 110.0;
const RIGHT_CLICK_PROB: f64 = 0.03;
const OFFSET_FRACTION: f64 = 0.30;
const OFFSET_EPSILON: f64 = 1e-6;
const SEED_HOVER: u64 = 0x9E37_79B9_7F4A_7C15;
const SEED_PRESS: u64 = 0x6A09_E667_F3BC_C908;
const SEED_OFFSET: u64 = 0xBB67_AE85_84CA_A73B;
const SEED_RIGHT: u64 = 0xA409_3822_299F_31D0;
const SEED_DBL_GAP: u64 = 0x510E_527F_ADE6_824D;
const SEED_DBL_DECIDE: u64 = 0x84CF_E392_4F1A_6B7E;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MouseButton {
    Left,
    Middle,
    Right,
    Back,
    Forward,
}

impl MouseButton {
    #[inline]
    pub fn button_code(self) -> u8 {
        match self {
            MouseButton::Left => 0,
            MouseButton::Middle => 1,
            MouseButton::Right => 2,
            MouseButton::Back => 3,
            MouseButton::Forward => 4,
        }
    }

    #[inline]
    pub fn buttons_bitmask(self) -> u16 {
        match self {
            MouseButton::Left => 1,
            MouseButton::Right => 2,
            MouseButton::Middle => 4,
            MouseButton::Back => 8,
            MouseButton::Forward => 16,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PointerType {
    Mouse,
    Pen,
    Touch,
}

impl PointerType {
    #[inline]
    pub fn as_str(self) -> &'static str {
        match self {
            PointerType::Mouse => "mouse",
            PointerType::Pen => "pen",
            PointerType::Touch => "touch",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum EventKind {
    PointerDown,
    PointerUp,
    MouseDown,
    MouseUp,
    Click,
    DoubleClick,
    ContextMenu,
    Focus,
    Blur,
    FocusIn,
    FocusOut,
    Wheel,
    VisibilityChange,
    KeyDown,
    KeyUp,
    TextInput,
}

#[derive(Debug, Clone, Copy)]
pub struct EventSpec {
    pub kind: EventKind,
    pub dt_ms: u32,
    pub button: u8,
    pub buttons: u16,
    pub detail: u32,
    pub pressure: f64,
    pub tilt_x: f64,
    pub tilt_y: f64,
    pub twist: f64,
    pub width: f64,
    pub height: f64,
    pub pointer_type: PointerType,
    pub is_primary: bool,
    pub is_trusted: bool,
}

impl EventSpec {
    fn base(kind: EventKind, dt_ms: u32) -> Self {
        Self {
            kind,
            dt_ms,
            button: 255,
            buttons: 0,
            detail: 0,
            pressure: 0.0,
            tilt_x: 0.0,
            tilt_y: 0.0,
            twist: 0.0,
            width: 0.0,
            height: 0.0,
            pointer_type: PointerType::Mouse,
            is_primary: false,
            is_trusted: true,
        }
    }

    pub fn mouse_button(kind: EventKind, button: MouseButton, buttons: u16, detail: u32, dt_ms: u32) -> Self {
        Self {
            button: button.button_code(),
            buttons,
            detail,
            width: 1.0,
            height: 1.0,
            is_primary: true,
            ..Self::base(kind, dt_ms)
        }
    }

    pub fn pointer(kind: EventKind, button: MouseButton, buttons: u16, detail: u32, dt_ms: u32, pressure: f64) -> Self {
        Self {
            pressure,
            ..Self::mouse_button(kind, button, buttons, detail, dt_ms)
        }
    }

    pub fn focus_like(kind: EventKind, dt_ms: u32) -> Self {
        Self::base(kind, dt_ms)
    }
}

#[derive(Debug, Clone)]
pub struct ClickPlan {
    pub events: Vec<EventSpec>,
    pub offset_x: f64,
    pub offset_y: f64,
}

impl ClickPlan {
    pub fn total_duration_ms(&self) -> u32 {
        self.events.iter().map(|e| e.dt_ms).sum()
    }
}

#[inline]
pub fn should_hover(seed: u64) -> bool {
    SplitMix64Rng::new(seed).next_f64() < HOVER_PROB
}

#[inline]
pub fn should_double_click(seed: u64) -> bool {
    SplitMix64Rng::new(seed.wrapping_add(SEED_DBL_DECIDE)).next_f64() < DOUBLE_CLICK_PROB
}

#[inline]
pub fn should_right_click(seed: u64) -> bool {
    SplitMix64Rng::new(seed.wrapping_add(SEED_RIGHT)).next_f64() < RIGHT_CLICK_PROB
}

#[inline]
pub fn hover_duration_ms(seed: u64) -> u32 {
    SplitMix64Rng::new(seed.wrapping_add(SEED_HOVER)).lognormal_ms(HOVER_MEDIAN_MS, 0.3)
}

#[inline]
pub fn press_duration_ms(seed: u64) -> u32 {
    SplitMix64Rng::new(seed.wrapping_add(SEED_PRESS)).lognormal_ms(PRESS_MEDIAN_MS, 0.26)
}

#[inline]
pub fn double_click_gap_ms(seed: u64) -> u32 {
    SplitMix64Rng::new(seed.wrapping_add(SEED_DBL_GAP)).lognormal_ms(DOUBLE_CLICK_GAP_MEDIAN_MS, 0.22)
}

pub fn click_offset(button_w: f64, button_h: f64, seed: u64) -> (f64, f64) {
    let mut rng = SplitMix64Rng::new(seed.wrapping_add(SEED_OFFSET));
    let max_dx = button_w * OFFSET_FRACTION;
    let max_dy = button_h * OFFSET_FRACTION;
    let dx = (rng.next_f64() * 2.0 - 1.0) * max_dx;
    let dy = (rng.next_f64() * 2.0 - 1.0) * max_dy;
    if dx.abs() < OFFSET_EPSILON && dy.abs() < OFFSET_EPSILON {
        (max_dx * 0.15, max_dy * 0.15)
    } else {
        (dx, dy)
    }
}

pub fn plan_click(
    focusable: bool,
    button: MouseButton,
    detail: u32,
    hover_ms: u32,
    press_ms: u32,
) -> ClickPlan {
    let mut events: Vec<EventSpec> = Vec::with_capacity(8);
    let mut dt = 0u32;
    if hover_ms > 0 {
        events.push(EventSpec::focus_like(EventKind::FocusIn, 0));
        events.push(EventSpec::focus_like(EventKind::Focus, 0));
        dt = hover_ms;
    }
    let down_buttons = button.buttons_bitmask();
    events.push(EventSpec::pointer(
        EventKind::PointerDown,
        button,
        down_buttons,
        detail,
        dt,
        0.5,
    ));
    events.push(EventSpec::mouse_button(
        EventKind::MouseDown,
        button,
        down_buttons,
        detail,
        0,
    ));
    events.push(EventSpec::pointer(EventKind::PointerUp, button, 0, detail, press_ms, 0.0));
    events.push(EventSpec::mouse_button(EventKind::MouseUp, button, 0, detail, 0));
    let click_dt = if button == MouseButton::Right { 0 } else { 0 };
    events.push(EventSpec::mouse_button(
        if button == MouseButton::Right {
            EventKind::ContextMenu
        } else if detail >= 2 {
            EventKind::DoubleClick
        } else {
            EventKind::Click
        },
        button,
        0,
        detail,
        click_dt,
    ));
    if focusable {
        events.push(EventSpec::focus_like(EventKind::Focus, 0));
    }
    ClickPlan {
        events,
        offset_x: 0.0,
        offset_y: 0.0,
    }
}

pub fn plan_human_click(
    button_w: f64,
    button_h: f64,
    focusable: bool,
    button: MouseButton,
    seed: u64,
) -> ClickPlan {
    let (dx, dy) = click_offset(button_w, button_h, seed);
    let hover_ms = if should_hover(seed) { hover_duration_ms(seed) } else { 0 };
    let press_ms = press_duration_ms(seed);
    let detail = if should_double_click(seed) { 2 } else { 1 };
    let mut plan = plan_click(focusable, button, detail, hover_ms, press_ms);
    if detail == 2 {
        let gap = double_click_gap_ms(seed);
        let mut second = plan_click(focusable, button, 2, 0, press_duration_ms(seed ^ 0x77));
        if let Some(first_up) = second.events.first_mut() {
            first_up.dt_ms = gap;
        }
        plan.events.append(&mut second.events);
    }
    plan.offset_x = dx;
    plan.offset_y = dy;
    plan
}

pub fn plan_blur_after_click(session_ms: u64, blur_count: u32, seed: u64) -> Option<u32> {
    if session_ms < 1500 || blur_count > 0 {
        return None;
    }
    Some(SplitMix64Rng::new(seed ^ 0xB1DA_0000_0000_0001).lognormal_ms(2400.0, 0.35))
}
