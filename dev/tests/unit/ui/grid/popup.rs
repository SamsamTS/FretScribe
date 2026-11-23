use super::*;

fn measure() -> Measure {
    Measure::new(7, 0.0, 100.0, TimeSignature::COMMON)
}

fn settings() -> MeasureSettings {
    MeasureSettings::new(2, &measure(), NotePolicy::FollowGrid)
}

#[test]
fn starts_from_the_measure_with_propagation_on() {
    let s = settings();
    assert_eq!((s.bpm, s.time_sig, s.measure_id, s.index), (100.0, TimeSignature::COMMON, 7, 2));
    assert!(s.apply_to_following);
    assert_eq!(s.propagation(), Propagation::Following);
    assert!(!s.is_changed());
    assert!(s.events().is_empty());
}

#[test]
fn only_changed_fields_produce_events() {
    let mut s = settings();
    s.bpm = 90.0;
    assert_eq!(
        s.events(),
        [GridEvent::TempoChanged {
            index: 2,
            bpm: 90.0,
            propagation: Propagation::Following,
            policy: NotePolicy::FollowGrid,
        }]
    );
    s.bpm = 100.0;
    s.time_sig = TimeSignature::new(6, 8);
    assert!(matches!(s.events()[..], [GridEvent::TimeSignatureChanged { index: 2, .. }]));
    s.bpm = 80.0;
    assert_eq!(s.events().len(), 2);
}

#[test]
fn checkbox_selects_single_or_following() {
    let mut s = settings();
    s.bpm = 90.0;
    s.apply_to_following = false;
    assert!(matches!(
        s.events()[..],
        [GridEvent::TempoChanged { propagation: Propagation::Single, .. }]
    ));
}

#[test]
fn policy_choice_is_carried_by_the_events() {
    let mut s = settings();
    s.bpm = 90.0;
    s.policy = NotePolicy::KeepAudioPosition;
    assert!(matches!(
        s.events()[..],
        [GridEvent::TempoChanged { policy: NotePolicy::KeepAudioPosition, .. }]
    ));
}

#[test]
fn invalid_input_cannot_be_confirmed() {
    let mut s = settings();
    s.bpm = 5.0;
    assert!(!s.is_valid() && s.events().is_empty());
    s.bpm = f64::NAN;
    assert!(!s.is_valid());
    let mut s = settings();
    s.time_sig = TimeSignature::new(4, 3);
    assert!(!s.is_valid() && s.events().is_empty());
}

#[test]
fn reverting_the_values_is_not_a_change() {
    let mut s = settings();
    s.bpm = 120.0;
    s.time_sig = TimeSignature::new(3, 4);
    assert!(s.is_changed());
    s.bpm = 100.0;
    s.time_sig = TimeSignature::COMMON;
    assert!(!s.is_changed());
}

#[test]
fn every_denominator_offered_is_a_valid_signature() {
    assert!(DENOMINATORS.iter().all(|&d| TimeSignature::new(4, d).is_valid()));
}
