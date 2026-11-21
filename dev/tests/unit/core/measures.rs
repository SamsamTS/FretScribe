use super::*;

pub fn close(a: f64, b: f64) {
    assert!((a - b).abs() < 1e-9, "{a} != {b}");
}

/// Equal to within half a tick (a position stored as ticks is at most that far from the time it came from).
pub fn near(a: f64, b: f64) {
    assert!((a - b).abs() < 0.011, "{a} != {b}");
}

/// Start and end seconds of a note.
pub fn secs(p: &crate::model::Project, id: u64) -> (f64, f64) {
    let times = crate::tempo::NoteTimes::build(&p.notes, &crate::tempo::TempoMap::new(&p.measures));
    (times.start_sec(id).unwrap(), times.end_sec(id).unwrap())
}

pub fn grid(tempos: &[f64]) -> Vec<Measure> {
    let mut ms: Vec<Measure> = tempos
        .iter()
        .enumerate()
        .map(|(i, &bpm)| Measure::new(i as u64 + 1, 0.0, bpm, TimeSignature::COMMON))
        .collect();
    recompute_starts(&mut ms, 1);
    ms
}

#[test]
fn cover_adds_and_removes_trailing_measures() {
    let mut ms = grid(&[120.0, 120.0]);
    let mut id = 10;
    assert!(cover_audio(&mut ms, 7.0, &mut id, |_| false));
    assert_eq!((ms.len(), id), (4, 12));
    close(ms[3].start_sec, 6.0);
    assert!(!cover_audio(&mut ms, 7.0, &mut id, |_| false));
    assert!(cover_audio(&mut ms, 3.0, &mut id, |m| m.id == 10));
    assert_eq!(ms.len(), 3);
    assert!(cover_audio(&mut ms, 0.5, &mut id, |_| false));
    assert_eq!(ms.len(), 1);
}

#[test]
fn generate_covers_audio_with_fresh_ids() {
    let mut next = 10;
    let ms = generate_measures(0.5, 10.0, 120.0, TimeSignature::COMMON, &mut next);
    assert_eq!(ms.len(), 5);
    assert_eq!(next, 15);
    assert_eq!(
        ms.iter().map(|m| m.id).collect::<Vec<_>>(),
        [10, 11, 12, 13, 14]
    );
    close(ms[0].start_sec, 0.5);
    close(ms[4].start_sec, 8.5);
    assert!(ms.iter().all(|m| !m.anchored));
    let last_end = ms[4].start_sec + ms[4].nominal_duration_sec();
    assert!(last_end >= 10.0);
}

#[test]
fn generate_exact_fit_has_no_extra_measure() {
    let mut next = 1;
    let ms = generate_measures(0.0, 8.0, 120.0, TimeSignature::COMMON, &mut next);
    assert_eq!(ms.len(), 4);
}

#[test]
fn generate_degenerate_inputs() {
    let mut next = 1;
    assert_eq!(
        generate_measures(0.0, 0.0, 120.0, TimeSignature::COMMON, &mut next).len(),
        1
    );
    assert_eq!(
        generate_measures(5.0, 1.0, 120.0, TimeSignature::COMMON, &mut next).len(),
        1
    );
    let ms = generate_measures(0.0, 4.0, 0.0, TimeSignature::new(0, 3), &mut next);
    assert_eq!(ms[0].time_sig, TimeSignature::COMMON);
    assert_eq!(ms[0].tempo_bpm, MIN_BPM);
    let ms = generate_measures(0.0, 4.0, f64::NAN, TimeSignature::COMMON, &mut next);
    assert_eq!(ms[0].tempo_bpm, 120.0);
}

#[test]
fn generate_other_signatures() {
    let mut next = 1;
    let ms = generate_measures(0.0, 6.0, 120.0, TimeSignature::new(6, 8), &mut next);
    assert_eq!(ms.len(), 4);
    close(ms[1].start_sec, 1.5);
}

#[test]
fn tempo_edit_propagates_to_all_following_measures() {
    let mut ms = grid(&[120.0, 120.0, 120.0, 90.0, 120.0]);
    let range = set_tempo(&mut ms, 0, 60.0).unwrap();
    assert_eq!(range, 0..5);
    let tempos: Vec<f64> = ms.iter().map(|m| m.tempo_bpm).collect();
    assert_eq!(tempos, [60.0; 5]);
}

#[test]
fn single_tempo_edit_leaves_equal_followers_alone() {
    let mut ms = grid(&[120.0, 120.0, 120.0]);
    let range = set_tempo_scoped(&mut ms, 0, 60.0, Propagation::Single).unwrap();
    assert_eq!(range, 0..1);
    let tempos: Vec<f64> = ms.iter().map(|m| m.tempo_bpm).collect();
    assert_eq!(tempos, [60.0, 120.0, 120.0]);
    close(ms[1].start_sec, 4.0);
    close(ms[2].start_sec, 6.0);
    assert_eq!(
        set_tempo_scoped(&mut ms, 0, 60.0, Propagation::Following).unwrap(),
        0..3
    );
}

#[test]
fn single_time_sig_edit_leaves_equal_followers_alone() {
    let mut ms = grid(&[120.0; 3]);
    let range =
        set_time_sig_scoped(&mut ms, 1, TimeSignature::new(3, 4), Propagation::Single).unwrap();
    assert_eq!(range, 1..2);
    assert_eq!(ms[1].time_sig, TimeSignature::new(3, 4));
    assert_eq!(ms[2].time_sig, TimeSignature::COMMON);
    close(ms[2].start_sec, 3.5);
    assert!(
        set_time_sig_scoped(&mut ms, 0, TimeSignature::new(4, 3), Propagation::Single).is_err()
    );
}

#[test]
fn tempo_edit_overrides_different_followers() {
    let mut ms = grid(&[120.0, 120.0, 90.0, 120.0]);
    set_tempo(&mut ms, 0, 100.0).unwrap();
    let tempos: Vec<f64> = ms.iter().map(|m| m.tempo_bpm).collect();
    assert_eq!(tempos, [100.0; 4]);
}

#[test]
fn tempo_edit_in_the_middle_leaves_earlier_measures_alone() {
    let mut ms = grid(&[120.0, 120.0, 120.0, 120.0]);
    let range = set_tempo(&mut ms, 2, 240.0).unwrap();
    assert_eq!(range, 2..4);
    let tempos: Vec<f64> = ms.iter().map(|m| m.tempo_bpm).collect();
    assert_eq!(tempos, [120.0, 120.0, 240.0, 240.0]);
    close(ms[2].start_sec, 4.0);
    close(ms[3].start_sec, 5.0);
}

#[test]
fn single_tempo_edit_between_different_neighbours() {
    let mut ms = grid(&[90.0, 120.0, 100.0]);
    assert_eq!(set_tempo_scoped(&mut ms, 1, 60.0, Propagation::Single).unwrap(), 1..2);
    let tempos: Vec<f64> = ms.iter().map(|m| m.tempo_bpm).collect();
    assert_eq!(tempos, [90.0, 60.0, 100.0]);
}

#[test]
fn tempo_edit_shifts_later_starts() {
    let mut ms = grid(&[120.0, 120.0, 90.0, 120.0]);
    set_tempo(&mut ms, 0, 60.0).unwrap();
    close(ms[1].start_sec, 4.0);
    close(ms[2].start_sec, 8.0);
    close(ms[3].start_sec, 12.0);
}

#[test]
fn a_manual_tempo_or_time_signature_change_drops_the_fit() {
    let mut ms = grid(&[120.0; 4]);
    ms.iter_mut().for_each(|m| m.anchored = true);
    set_tempo_scoped(&mut ms, 1, 120.0, Propagation::Following).unwrap();
    assert!(ms.iter().all(|m| m.anchored), "unchanged value keeps the fit");
    set_tempo_scoped(&mut ms, 1, 90.0, Propagation::Single).unwrap();
    assert_eq!(ms.iter().map(|m| m.anchored).collect::<Vec<_>>(), [true, false, true, true]);
    set_time_sig(&mut ms, 2, TimeSignature::new(3, 4)).unwrap();
    assert_eq!(ms.iter().map(|m| m.anchored).collect::<Vec<_>>(), [true, false, false, false]);
}

#[test]
fn anchored_measures_keep_their_start() {
    let mut ms = grid(&[120.0, 120.0, 120.0, 120.0]);
    ms[2].anchored = true;
    set_tempo_scoped(&mut ms, 0, 60.0, Propagation::Single).unwrap();
    close(ms[1].start_sec, 4.0);
    close(ms[2].start_sec, 4.0);
    close(ms[3].start_sec, 6.0);
}

#[test]
fn anchor_on_the_edited_measure_does_not_block_its_own_edit() {
    let mut ms = grid(&[120.0, 120.0]);
    ms[0].anchored = true;
    set_tempo(&mut ms, 0, 60.0).unwrap();
    close(ms[0].start_sec, 0.0);
    close(ms[1].start_sec, 4.0);
}

#[test]
fn tempo_validation() {
    let mut ms = grid(&[120.0]);
    assert_eq!(
        set_tempo(&mut ms, 0, 0.0),
        Err(EditError::InvalidTempo(0.0))
    );
    assert!(set_tempo(&mut ms, 0, f64::NAN).is_err());
    assert!(set_tempo(&mut ms, 0, 9.0).is_err());
    assert!(set_tempo(&mut ms, 0, 501.0).is_err());
    assert_eq!(
        set_tempo(&mut ms, 3, 100.0),
        Err(EditError::MeasureOutOfRange { index: 3, len: 1 })
    );
    assert_eq!(ms[0].tempo_bpm, 120.0);
}

#[test]
fn time_sig_edit_propagates_to_all_following_measures() {
    let mut ms = grid(&[120.0; 5]);
    ms[3].time_sig = TimeSignature::new(3, 4);
    ms[4].time_sig = TimeSignature::new(4, 4);
    let range = set_time_sig(&mut ms, 1, TimeSignature::new(6, 8)).unwrap();
    assert_eq!(range, 1..5);
    let sigs: Vec<(u8, u8)> = ms
        .iter()
        .map(|m| (m.time_sig.numerator, m.time_sig.denominator))
        .collect();
    assert_eq!(sigs, [(4, 4), (6, 8), (6, 8), (6, 8), (6, 8)]);
}

#[test]
fn time_sig_edit_recomputes_starts() {
    let mut ms = grid(&[120.0; 4]);
    set_time_sig(&mut ms, 1, TimeSignature::new(3, 4)).unwrap();
    close(ms[1].start_sec, 2.0);
    close(ms[2].start_sec, 3.5);
    close(ms[3].start_sec, 5.0);
}

#[test]
fn time_sig_edit_respects_anchors() {
    let mut ms = grid(&[120.0; 4]);
    ms[3].anchored = true;
    set_time_sig_scoped(&mut ms, 0, TimeSignature::new(3, 4), Propagation::Single).unwrap();
    close(ms[2].start_sec, 3.5);
    close(ms[3].start_sec, 6.0);
}

#[test]
fn time_sig_validation() {
    let mut ms = grid(&[120.0]);
    assert!(set_time_sig(&mut ms, 0, TimeSignature::new(0, 4)).is_err());
    assert!(set_time_sig(&mut ms, 0, TimeSignature::new(4, 3)).is_err());
    assert!(set_time_sig(&mut ms, 0, TimeSignature::new(4, 0)).is_err());
    assert_eq!(ms[0].time_sig, TimeSignature::COMMON);
}

