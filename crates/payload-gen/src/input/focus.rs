const RAPID_FOCUS_SWITCH_MS: u64 = 80;
const MIN_SESSION_MS: u64 = 1500;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum FocusState {
    Focused,
    Blurred,
    Visible,
    Hidden,
}

impl FocusState {
    pub fn as_str(self) -> &'static str {
        match self {
            FocusState::Focused => "focused",
            FocusState::Blurred => "blurred",
            FocusState::Visible => "visible",
            FocusState::Hidden => "hidden",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum FocusAnomaly {
    NeverBlurred,
    RapidFocusSwitching,
    NoVisibilityChange,
}

pub fn is_focusable_target(tag: &str, attrs: &[(&str, &str)]) -> bool {
    let tag = tag.to_ascii_lowercase();
    if matches!(
        tag.as_str(),
        "input" | "button" | "textarea" | "select" | "summary" | "details"
    ) {
        if tag == "input" {
            let t = attrs.iter().find_map(|(k, v)| (*k == "type").then_some(*v));
            return !matches!(t, Some("hidden"));
        }
        return true;
    }
    if tag == "a" {
        return attrs.iter().any(|(k, _)| *k == "href");
    }
    attrs.iter().any(|(k, _)| *k == "tabindex")
}

pub fn is_rapid_focus_switch(prev_focus_us: u64, blur_us: u64) -> bool {
    blur_us.saturating_sub(prev_focus_us) <= RAPID_FOCUS_SWITCH_MS * 1000
}

pub fn detect_focus_anomalies(events: &[(FocusState, u64)], session_ms: u64) -> Vec<FocusAnomaly> {
    let mut out = Vec::new();
    let blur_count = events.iter().filter(|(s, _)| *s == FocusState::Blurred).count();
    if session_ms >= MIN_SESSION_MS && blur_count == 0 {
        out.push(FocusAnomaly::NeverBlurred);
    }
    let vis_count = events
        .iter()
        .filter(|(s, _)| matches!(s, FocusState::Hidden | FocusState::Visible))
        .count();
    if session_ms >= MIN_SESSION_MS && vis_count == 0 {
        out.push(FocusAnomaly::NoVisibilityChange);
    }
    for w in events.windows(2) {
        if w[0].0 == FocusState::Focused
            && w[1].0 == FocusState::Blurred
            && is_rapid_focus_switch(w[0].1, w[1].1)
        {
            out.push(FocusAnomaly::RapidFocusSwitching);
            break;
        }
    }
    out
}
