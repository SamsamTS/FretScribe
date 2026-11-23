use super::*;

fn state() -> FormState {
    FormState::new("/m/song.flac".into(), "Song".into(), true)
}

#[test]
fn create_disabled_until_valid() {
    let mut s = state();
    s.set_main_readable(false);
    assert!(s.validated().unwrap_err().main_file.is_some());
    s.set_main_readable(true);
    for (edit, bad) in [("name", ""), ("bpm", "5"), ("time_sig", "4/3"), ("reference_hz", "500"), ("tuning", "E A")] {
        let mut t = state();
        match edit {
            "name" => t.values.name = bad.into(),
            "bpm" => t.values.bpm = bad.into(),
            "time_sig" => t.values.time_sig = bad.into(),
            "reference_hz" => t.values.reference_hz = bad.into(),
            _ => t.values.tuning = bad.into(),
        }
        assert!(t.validated().is_err(), "{edit}");
    }
}

#[test]
fn instrument_switch_rewrites_tuning_once() {
    let mut s = state();
    s.set_instrument(Instrument::Bass);
    assert_eq!(s.values.tuning, "E A D g");
    s.values.tuning = "D A D G".into();
    s.set_instrument(Instrument::Bass);
    assert_eq!(s.values.tuning, "D A D G");
    assert_eq!(s.validated().unwrap().tuning.open_strings.len(), 4);
}

#[test]
fn preset_texts_are_valid_for_the_instrument() {
    let mut s = state();
    for instrument in [Instrument::Guitar, Instrument::Bass] {
        s.set_instrument(instrument);
        for (_, text) in preset_texts(instrument) {
            s.values.tuning = text;
            assert!(s.validated().is_ok());
        }
    }
}

#[test]
fn estimates_never_write_fields() {
    let mut s = state();
    s.values.bpm = "93".into();
    s.set_bpm_candidates(vec![(118.0, 0.9), (59.0, 0.4)]);
    s.set_reference_estimate(4.0, 441.0, 0.8);
    s.set_estimating(true);
    assert_eq!((s.values.bpm.as_str(), s.values.reference_hz.as_str()), ("93", "440.0"));
    assert_eq!((chip_text(118.0), field_text(441.0)), ("~118".to_owned(), "441.0".to_owned()));
    assert_eq!(reference_hint(4.0, 0.8), "Main track: +4 cents");
    assert!(reference_hint(-12.0, 0.1).ends_with("(low confidence)"));
}

#[test]
fn tracks_add_remove_and_dedupe() {
    let mut s = state();
    s.add_track("/m/drums.wav".into());
    s.add_track("/m/other.wav".into());
    s.add_track("/m/drums.wav".into());
    s.add_track("/m/song.flac".into());
    let roles: Vec<_> = s.tracks().iter().map(|t| t.1).collect();
    assert_eq!(roles, [TrackRole::Guitar, TrackRole::Drums, TrackRole::Other]);
    s.values.extra.remove(0);
    assert_eq!(s.tracks().len(), 2);
    assert_eq!(s.validated().unwrap().extra.len(), 1);
}

