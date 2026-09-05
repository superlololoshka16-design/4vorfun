use payload_gen::input::{
    Calibration, ClickCursor, InputHub, MotionCursor, Persona, RawEvent, ScrollCursor,
    TabInput, TypingCursor, events_bytes,
};

fn run_motion(seed: u64, trust: i32) -> (Vec<RawEvent>, bool) {
    let persona = Persona::derive(0xA11CE, 7);
    let mut cur = MotionCursor::new(
        persona,
        120.0,
        90.0,
        640.0,
        400.0,
        36.0,
        1_000_000,
        seed,
        trust,
    );
    let mut events = Vec::new();
    let mut now = 1_000_000u64;
    while !cur.done() && now < 60_000_000 {
        if let Some(ev) = cur.step(now) {
            events.push(ev);
        }
        now = cur.next_due_us().max(now + 1);
    }
    let on = cur.on_target();
    (events, on)
}

#[test]
fn motion_reaches_target_with_variable_timing() {
    let (events, on) = run_motion(0x5EED_0001, 0);
    assert!(on, "движение обязано закончиться в цели");
    assert!(events.len() > 15, "полёт на 600px не бывает 5 точками: {}", events.len());
    let dts: Vec<u16> = events.iter().map(|e| e.dt_ms).collect();
    let uniq = dts.iter().collect::<std::collections::HashSet<_>>().len();
    assert!(uniq > 3, "константный дельта-тайм = маркер бота, уников: {uniq}");
    assert!(dts.iter().all(|&d| d > 0));
    let mut peak = 0.0f64;
    for w in events.windows(2) {
        let dx = (w[1].x as f64 - w[0].x as f64).abs();
        let dy = (w[1].y as f64 - w[0].y as f64).abs();
        let step = dx.hypot(dy) / w[1].dt_ms.max(1) as f64;
        if step > peak {
            peak = step;
        }
    }
    assert!(peak > 0.0, "нулевая скорость на протяжении пути");
}

#[test]
fn motion_shape_is_not_a_fixed_template() {
    let frac = |seed: u64| {
        let (events, _) = run_motion(seed, 0);
        let n = events.len().max(1);
        let mut peak_i = 0usize;
        let mut peak = 0.0f64;
        for (i, w) in events.windows(2).enumerate() {
            let dx = (w[1].x as f64 - w[0].x as f64).abs();
            let dy = (w[1].y as f64 - w[0].y as f64).abs();
            let step = dx.hypot(dy) / w[1].dt_ms.max(1) as f64;
            if step > peak {
                peak = step;
                peak_i = i;
            }
        }
        peak_i as f64 / n as f64
    };
    let f1 = frac(0x5EED_0002);
    let f2 = frac(0x5EED_0003);
    let f3 = frac(0x5EED_0004);
    assert!(
        (f1 - f2).abs() > 0.02 || (f2 - f3).abs() > 0.02,
        "пик скорости у всех сидов в одной точке = фиксированный шаблон фаз: {f1} {f2} {f3}"
    );
}

#[test]
fn motion_is_deterministic_per_seed() {
    let (a, _) = run_motion(0x5EED_0005, 0);
    let (b, _) = run_motion(0x5EED_0005, 0);
    assert_eq!(a.len(), b.len());
    assert!(a.iter().zip(b.iter()).all(|(x, y)| x.x == y.x && x.y == y.y && x.dt_ms == y.dt_ms));
}

#[test]
fn persona_is_stable_and_distinct_per_tab() {
    let p1 = Persona::derive(0xBEEF, 1);
    let p1_again = Persona::derive(0xBEEF, 1);
    let p2 = Persona::derive(0xBEEF, 2);
    assert_eq!(p1.poll_hz, p1_again.poll_hz);
    assert_eq!(p1.wpm, p1_again.wpm);
    assert_ne!(p1.wpm, p2.wpm, "вкладки обязаны отличаться моторикой");
}

#[test]
fn typing_has_burst_rhythm_not_flat_gaps() {
    let persona = Persona::derive(0xC0FFEE, 3);
    let mut cur = TypingCursor::new(persona, "the quick brown fox jumps over the lazy dog", 0, 0x7EED, 0);
    let mut gaps: Vec<u64> = Vec::new();
    let mut now = 0u64;
    while !cur.done() && now < 60_000_000 {
        let due = cur.next_due_us();
        if due > now {
            now = due;
        }
        if cur.step(now).is_some() {
            gaps.push(cur.next_due_us() - now);
        } else if cur.done() {
            break;
        }
    }
    assert!(gaps.len() > 30);
    let mut sorted = gaps.clone();
    sorted.sort_unstable();
    let median = sorted[sorted.len() / 2];
    let max = *sorted.last().unwrap();
    assert!(
        max > median * 3,
        "без бёрстов все интервалы плоские: медиана {median}, максимум {max}"
    );
}

#[test]
fn click_dwell_is_lognormal_band() {
    let persona = Persona::derive(0xD0D0, 9);
    let mut holds = Vec::new();
    for i in 0..40 {
        let mut cur = ClickCursor::new(persona, 300, 200, 1_000_000, 0x9E37 + i as u64, 0);
        let mut now = 1_000_000u64;
        let mut press_t = 0u64;
        let mut release_t = 0u64;
        while !cur.done() && now < 5_000_000 {
            if let Some(ev) = cur.step(now) {
                if ev.kind == payload_gen::input::input::PRESS {
                    press_t = now;
                }
                if ev.kind == payload_gen::input::input::RELEASE {
                    release_t = now;
                }
            }
            now = cur.next_due_us().max(now + 1);
        }
        holds.push(release_t.saturating_sub(press_t) / 1000);
    }
    let mut sorted = holds.clone();
    sorted.sort_unstable();
    let min = *sorted.first().unwrap();
    let max = *sorted.last().unwrap();
    let p05 = sorted[sorted.len() / 20];
    let p95 = sorted[sorted.len() - 1 - sorted.len() / 20];
    assert!(min >= 25, "dwell ниже 25ms физически невозможен: {min}");
    assert!(max <= 500, "dwell выше 500ms палится: {max}");
    assert!(
        p05 >= 40 && p95 <= 380,
        "рабочая полоса dwell обязана быть человеческой: p05={p05} p95={p95}"
    );
    assert!(max > min, "константный dwell = маркер бота");
}

#[test]
fn scroll_decays_and_lands() {
    let persona = Persona::derive(0xE11, 4);
    let mut cur = ScrollCursor::new(persona, -900.0, 0, 0x5C20, 0);
    let mut deltas = Vec::new();
    let mut now = 0u64;
    let mut last_y = 0i32;
    while !cur.done() && now < 20_000_000 {
        if let Some(ev) = cur.step(now) {
            let y = ev.y as i32;
            deltas.push((y - last_y).abs());
            last_y = y;
        }
        now = cur.next_due_us().max(now + 1);
    }
    assert!(cur.done(), "скролл обязан доехать");
    assert!(deltas.len() > 3);
    let first = deltas.iter().take(3).sum::<i32>();
    let last = deltas.iter().rev().take(3).sum::<i32>();
    assert!(first > last, "трение обязано гасить скорость: {first} vs {last}");
}

#[test]
fn raw_event_is_packed_pod() {
    assert_eq!(payload_gen::input::RAW_EVENT_LEN, 8);
    let events = [
        RawEvent::new(11, 22, 33, 4, 5),
        RawEvent::new(66, 77, 88, 1, 2),
    ];
    let bytes = events_bytes(&events);
    assert_eq!(bytes.len(), 16);
    assert_eq!(&bytes[..2], &[11, 0]);
}

#[test]
fn hub_prioritises_weight_and_calibration_changes_logic() {
    let mut hub = InputHub::new();
    let site = hub.register_site(3);
    let weak = hub.open_tab(site, 1);
    let strong = hub.open_tab(site, 9);
    let persona = Persona::derive(0xF00D, 1);
    hub.set_input(
        weak,
        TabInput::Move(MotionCursor::new(persona, 0.0, 0.0, 500.0, 300.0, 30.0, 1_000, 0x1, 0)),
    );
    hub.set_input(
        strong,
        TabInput::Move(MotionCursor::new(persona, 0.0, 0.0, 500.0, 300.0, 30.0, 1_000, 0x2, 0)),
    );
    let mut out = smallvec::SmallVec::new();
    hub.tick(1_000, &mut out);
    assert!(!out.is_empty());
    assert_eq!(out[0].0, strong, "болящий вес берёт слот первым");
    assert_eq!(hub.live_tabs(), 2);

    let calib = Calibration::new(3);
    calib.record(false);
    calib.record(false);
    assert_eq!(calib.trust(), -8);
    assert_eq!(calib.stats().1, 2);
    let throttled = persona.throttle(calib.trust());
    assert!(throttled.corrections_max > persona.throttle(0).corrections_max);
}
