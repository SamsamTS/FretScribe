use super::parse_reference;

#[test]
fn reference_accepts_hz_and_cents_within_range() {
    assert_eq!(parse_reference("437.5"), Some(437.5));
    assert_eq!(parse_reference("442 Hz"), Some(442.0));
    let cents = parse_reference("-14c").unwrap();
    assert!((cents - 436.5).abs() < 0.2, "{cents}");
    assert_eq!(parse_reference("500"), None);
    assert_eq!(parse_reference("abc"), None);
}
