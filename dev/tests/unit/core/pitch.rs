use super::*;

fn r(hz: f32) -> PitchRef {
    PitchRef::new(hz).expect("valid reference")
}

#[test]
fn validation_range() {
    assert!(PitchRef::new(400.0).is_ok());
    assert!(PitchRef::new(480.0).is_ok());
    assert!(PitchRef::new(399.9).is_err());
    assert!(PitchRef::new(480.1).is_err());
    assert!(PitchRef::new(f32::NAN).is_err());
}

#[test]
fn serde_validates() {
    assert_eq!(serde_json::to_string(&r(432.0)).unwrap(), "432.0");
    assert_eq!(serde_json::from_str::<PitchRef>("442.0").unwrap(), r(442.0));
    assert!(serde_json::from_str::<PitchRef>("100.0").is_err());
}

#[test]
fn a4_is_midi_69_in_every_reference() {
    for hz in [440.0, 432.0, 442.0] {
        let p = r(hz);
        assert!((freq_to_midi_float(hz, &p) - 69.0).abs() < 1e-4);
        assert!((midi_to_freq(69.0, &p) - hz).abs() < 1e-3);
        assert_eq!(cents_off(hz, &p).0, 69);
        assert!(cents_off(hz, &p).1.abs() < 1e-2);
    }
}

#[test]
fn middle_c_across_references() {
    assert!((midi_to_freq(60.0, &r(440.0)) - 261.6256).abs() < 1e-3);
    assert!((midi_to_freq(60.0, &r(432.0)) - 256.8687).abs() < 1e-3);
    assert!((midi_to_freq(60.0, &r(442.0)) - 262.8138).abs() < 1e-3);
}

#[test]
fn round_trip_frequency_midi() {
    for hz in [400.0, 432.0, 440.0, 442.0, 480.0] {
        let p = r(hz);
        for midi in [28.0_f32, 40.0, 59.5, 64.0, 88.25] {
            let back = freq_to_midi_float(midi_to_freq(midi, &p), &p);
            assert!((back - midi).abs() < 1e-3, "{hz} {midi} {back}");
        }
    }
}

#[test]
fn cents_against_other_reference() {
    let (midi, cents) = cents_off(440.0, &r(432.0));
    assert_eq!(midi, 69);
    assert!((cents - 31.767).abs() < 0.01, "{cents}");
    let (midi, cents) = cents_off(440.0, &r(442.0));
    assert_eq!(midi, 69);
    assert!((cents + 7.85).abs() < 0.01, "{cents}");
}

#[test]
fn cents_arithmetic() {
    let p = r(440.0);
    let quarter_tone_up = midi_to_freq(60.5, &p);
    let (midi, cents) = cents_off(quarter_tone_up, &p);
    assert!(midi == 60 || midi == 61);
    assert!((cents.abs() - 50.0).abs() < 0.05);
    let (midi, cents) = cents_off(midi_to_freq(60.0, &p) * 2.0_f32.powf(10.0 / 1200.0), &p);
    assert_eq!(midi, 60);
    assert!((cents - 10.0).abs() < 0.02);
}

#[test]
fn cents_off_degenerate_input() {
    let p = PitchRef::default();
    assert_eq!(cents_off(0.0, &p), (0, 0.0));
    assert_eq!(cents_off(-5.0, &p), (0, 0.0));
    assert_eq!(cents_off(f32::NAN, &p), (0, 0.0));
}

#[test]
fn reference_cents_round_trip() {
    assert!((ref_from_cents(0.0).a4_hz() - 440.0).abs() < 1e-4);
    assert!((ref_from_cents(-31.767).a4_hz() - 432.0).abs() < 1e-2);
    assert!((ref_from_cents(7.8).a4_hz() - 442.0).abs() < 1e-1);
    for cents in [-100.0_f32, -31.0, 0.0, 12.5, 100.0] {
        assert!((cents_of(&ref_from_cents(cents)) - cents).abs() < 1e-2);
    }
    assert!(cents_of(&r(432.0)) < 0.0 && cents_of(&r(442.0)) > 0.0);
}

#[test]
fn reference_from_cents_is_clamped() {
    assert_eq!(ref_from_cents(1000.0).a4_hz(), 480.0);
    assert_eq!(ref_from_cents(-1000.0).a4_hz(), 400.0);
}

