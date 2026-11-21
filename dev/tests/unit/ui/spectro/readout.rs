use super::*;
use egui::pos2;

#[test]
fn readout_names_cents_frequency_and_candidate() {
    let std = Tuning::standard();
    let r = PitchRef::default();
    let midi = 58.0 + 0.12;
    let out = readout(midi, &r, &std, 0);
    assert!(out.pitch.starts_with("A#3 +12c, 23"), "{}", out.pitch);
    assert!(out.pitch.ends_with(" Hz"));
    assert_eq!(out.candidate.as_deref(), Some("G 3"));
    assert_eq!(readout(64.0, &r, &std, 0).candidate.as_deref(), Some("e 0"));
    assert_eq!(readout(30.0, &r, &std, 0).candidate, None);
}

#[test]
fn readout_is_relative_to_the_reference_pitch() {
    let tuned = PitchRef::new(432.0).unwrap();
    let out = readout(69.0, &tuned, &Tuning::standard(), 0);
    assert!(out.pitch.starts_with("A4 +0c, 432.0"), "{}", out.pitch);
}

#[test]
fn natural_notes_and_open_strings() {
    let naturals: Vec<i32> = (60..72).filter(|&m| is_natural(m)).collect();
    assert_eq!(naturals, [60, 62, 64, 65, 67, 69, 71]);
    assert!(is_natural(-12));
    assert_eq!(open_pitches(&Tuning::standard(), 2), [42, 47, 52, 57, 61, 66]);
}

#[test]
fn readout_never_leaves_the_bounds() {
    let b = Rect::from_min_size(pos2(0.0, 0.0), vec2(300.0, 200.0));
    let size = vec2(110.0, 34.0);
    for cursor in [pos2(10.0, 10.0), pos2(295.0, 10.0), pos2(10.0, 195.0), pos2(299.0, 199.0), pos2(150.0, 100.0)] {
        let p = place_readout(b, cursor, size);
        assert!(b.contains_rect(Rect::from_min_size(p, size)), "{cursor:?} -> {p:?}");
    }
    assert_eq!(place_readout(b, pos2(10.0, 10.0), size), pos2(24.0, 24.0));
    assert!(place_readout(b, pos2(295.0, 10.0), size).x < 295.0);
}
