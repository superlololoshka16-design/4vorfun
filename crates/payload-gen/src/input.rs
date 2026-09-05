pub mod activity;
pub mod click;
pub mod event;
pub mod focus;
pub mod lifecycle;
pub mod motion;
pub mod perlin;
pub mod persona;
pub mod prng;
pub mod scheduler;
pub mod session;
pub mod scroll;
pub mod trajectory;
pub mod typing;
pub mod windmouse;

pub use activity::{
    COGNITIVE_DELAY_LO_MS, COGNITIVE_DELAY_HI_MS, HAND_TREMOR_AMP_HI, HAND_TREMOR_AMP_LO,
    HAND_TREMOR_FREQ_HI, HAND_TREMOR_FREQ_LO, HEARTBEAT_BUMP_PX, HEARTBEAT_INTERVAL_MAX_MS,
    HEARTBEAT_INTERVAL_MIN_MS, IK_CLICK, IK_KEY_DOWN, IK_MOUSE_MOVE, IK_SCROLL,
    REQUEST_JITTER_MAX_MS, VS_HIDDEN, VS_VISIBLE, WINDMOUSE_DAMPED, WINDMOUSE_GRAVITY,
    WINDMOUSE_MAX_STEP, WINDMOUSE_WIND, RequestJitter, VisibilityFlip, cognitive_delay_ms,
    hand_tremor, input_safe, mouse_heartbeat, per_tab_seed, request_jitter, tab_rng,
    visibility_flip, visibility_state_str, wind_mouse_path, wrist_pivot_arc,
};
pub use click::{
    ClickAuthenticityProfile, ClickButton, ClickCursor, ClickEvent, ClickPlan,
    generate_click_timing,
};
pub use event::{RAW_EVENT_LEN, RawEvent, button, events_bytes, input};
pub use focus::{
    FocusAnomaly, FocusState, detect_focus_anomalies, is_focusable_target, is_rapid_focus_switch,
};
pub use lifecycle::{
    ClickPlan as LifecycleClickPlan, EventKind, EventSpec, MouseButton, PointerType,
    click_offset, double_click_gap_ms, hover_duration_ms, plan_blur_after_click, plan_click,
    plan_human_click, press_duration_ms, should_double_click, should_hover, should_right_click,
};
pub use motion::MotionCursor;
pub use perlin::Perlin2D;
pub use persona::{Persona, ThrottledPersona};
pub use prng::SplitMix64Rng;
pub use scheduler::{Calibration, InputHub, TabId, TabInput};
pub use session::{
    BATCH_CAP, BATCH_INTERVAL_US, TabPhase, TabSession, TabTick, TelemetryBatcher,
};
pub use scroll::{
    ScrollCursor, ScrollMethod, ScrollPath, ScrollStep, chunk_size_px, generate_scroll_path,
    reading_scroll_duration_ms, scroll_velocity_px_s, should_overscroll, should_overshoot,
};
pub use trajectory::{
    Trajectory, TrajectoryParams, TrajectoryPoint, base_path_windmouse, generate_heartbeat_bump,
    generate_idle_drift,
};
pub use typing::{
    TypingCursor, TypingStep, bigram_latency_ms, generate_typing_timeline, log_normal_delay_ms,
    sentence_pause_ms, should_backspace, should_capitalize_error, word_pause_ms,
};
pub use windmouse::{WindMouseParams, WindMouseStream, wind_mouse};
