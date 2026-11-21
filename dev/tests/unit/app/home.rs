use super::*;

#[test]
fn civil_dates() {
    assert_eq!(civil_from_days(0), (1970, 1, 1));
    assert_eq!(civil_from_days(-1), (1969, 12, 31));
    assert_eq!(civil_from_days(11_016), (2000, 2, 29));
    assert_eq!(civil_from_days(11_017), (2000, 3, 1));
    assert_eq!(civil_from_days(20_453), (2025, 12, 31));
    assert_eq!(civil_from_days(20_454), (2026, 1, 1));
    assert_eq!(civil_from_days(-719_468), (0, 3, 1));
}

#[test]
fn labels() {
    let now = 20_763 * SECS_PER_DAY + 72_000; // 2026-11-06 20:00 UTC
    assert_eq!(date_label(now - 3600, now), "Today");
    assert_eq!(date_label(now - SECS_PER_DAY, now), "Yesterday");
    assert_eq!(date_label(now - 5 * SECS_PER_DAY, now), "1 Nov 2026");
    assert_eq!(date_label(0, now), "");
    assert_eq!(date_label(now + 5 * SECS_PER_DAY, now), "11 Nov 2026");
}

#[test]
fn grid_columns() {
    assert_eq!(columns(10.0), 1);
    assert_eq!(columns(TILE), 1);
    assert_eq!(columns(2.0 * TILE + GAP), 2);
    assert_eq!(columns(2.0 * TILE + GAP - 1.0), 1);
    assert_eq!(columns(900.0), 4);
}

#[test]
fn logo_scales_down_but_keeps_aspect() {
    assert_eq!(logo_size(2000.0, 0.5), Vec2::new(LOGO_MAX_W, LOGO_MAX_W * 0.5));
    assert_eq!(logo_size(100.0, 0.5), Vec2::new(70.0, 35.0));
}

#[test]
fn bars_are_centred_and_non_empty() {
    let bars = thumb_bars(&[0, 255], 100.0, 40.0);
    assert_eq!(bars, [(25.0, 1.0), (75.0, 20.0)]);
    assert!(thumb_bars(&[], 100.0, 40.0).is_empty());
}
