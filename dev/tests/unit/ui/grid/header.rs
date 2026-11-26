use super::header::*;
use super::GridEvent;
use fretscribe_core::{Measure, NotePolicy, TimeSignature};

const FOLLOW: NotePolicy = NotePolicy::FollowGrid;

fn measures(tempos: &[f64]) -> Vec<Measure> {
    tempos.iter().enumerate().map(|(i, &t)| Measure::new(i as u64 + 1, i as f64 * 2.0, t, TimeSignature::COMMON)).collect()
}

#[test]
fn items_are_dropped_by_priority_as_a_strict_prefix() {
    use Item::{AutoFit, LetRing, Lock, Number, Overflow, Tempo, TimeSig};
    let widths = [30.0, 40.0, 80.0, 26.0, 26.0, 26.0, 26.0];
    let shown = |avail| Item::ALL.into_iter().filter(|&i| visible_items(avail, &widths)[i as usize]).collect::<Vec<_>>();
    assert_eq!(shown(45.0), []);
    assert_eq!(shown(46.0), [Number]);
    assert_eq!(shown(76.0), [Number, Overflow]);
    assert_eq!(shown(106.0), [Number, Lock, Overflow]);
    assert_eq!(shown(150.0), [Number, TimeSig, Lock, Overflow]);
    assert_eq!(shown(233.0), [Number, TimeSig, Lock, Overflow], "auto-fit would fit but tempo comes first");
    assert_eq!(shown(234.0), [Number, TimeSig, Tempo, Lock, Overflow]);
    assert_eq!(shown(264.0), [Number, TimeSig, Tempo, AutoFit, Lock, Overflow]);
    assert_eq!(shown(294.0), [Number, TimeSig, Tempo, AutoFit, LetRing, Lock, Overflow]);
}

#[test]
fn popup_text_is_validated() {
    let tempo = |t| Field::Tempo.event(t, 2, true, FOLLOW);
    assert_eq!(tempo(" 72.4 "), Some(GridEvent::TempoApplied { index: 2, bpm: 72.4, propagate: true, policy: FOLLOW }));
    assert!(["", "abc", "9", "501", "NaN"].into_iter().all(|t| tempo(t).is_none()));
    let sig = |t| Field::TimeSig.event(t, 0, false, FOLLOW);
    assert_eq!(sig("7 / 8"), Some(GridEvent::TimeSigApplied { index: 0, sig: TimeSignature::new(7, 8), propagate: false, policy: FOLLOW }));
    assert!(["", "4", "4/3", "0/4", "4/4/4", "300/4"].into_iter().all(|t| sig(t).is_none()));
}

#[test]
fn the_hint_counts_identical_followers_up_to_a_lock() {
    let mut ms = measures(&[70.0, 70.0, 70.0, 90.0, 70.0]);
    assert_eq!(Field::Tempo.following(&ms, 0), 2);
    assert_eq!(Field::TimeSig.following(&ms, 0), 4);
    ms[2].locked = true;
    assert_eq!(Field::Tempo.following(&ms, 0), 1);
    assert_eq!(Field::TimeSig.following(&ms, 0), 1);
}

#[test]
fn toggles_report_the_inverted_flag() {
    let m = &measures(&[120.0])[0];
    let event = |item, ctrl| toggle_event(item, 0, m, ctrl, FOLLOW);
    assert_eq!(event(Item::AutoFit, false), Some(GridEvent::AutoFitToggled { index: 0, on: false, propagate: false, policy: FOLLOW }));
    assert_eq!(event(Item::LetRing, true), Some(GridEvent::LetRingToggled { index: 0, on: true, propagate: true }));
    assert_eq!(event(Item::Lock, true), Some(GridEvent::LockToggled { index: 0, on: true, propagate: true }));
    assert_eq!(event(Item::Tempo, false), None);
}

#[test]
fn a_locked_measure_disables_signature_tempo_and_auto_fit() {
    let disabled: Vec<Item> = Item::ALL.into_iter().filter(|i| !i.enabled(true)).collect();
    assert_eq!(disabled, [Item::TimeSig, Item::Tempo, Item::AutoFit]);
    assert!(Item::ALL.into_iter().all(|i| i.enabled(false)));
}

#[test]
fn confidence_colours_and_fitted_tempo() {
    use crate::theme::{CONFIDENCE_HIGH, CONFIDENCE_LOW, CONFIDENCE_MID};
    for (c, colour) in [(0.8, CONFIDENCE_HIGH), (0.79, CONFIDENCE_MID), (0.5, CONFIDENCE_MID), (0.49, CONFIDENCE_LOW)] {
        assert_eq!(confidence_color(c), colour, "{c}");
    }
    let fit = |tempo_bpm| FitInfo { tempo_bpm, end_sec: 2.0, confidence: 1.0 };
    assert_eq!(fitted_suffix(70.0, Some(&fit(72.4))).as_deref(), Some("(72.4)"));
    assert_eq!(fitted_suffix(70.0, Some(&fit(70.5))).as_deref(), Some("(70.5)"));
    assert_eq!(fitted_suffix(70.0, Some(&fit(70.49))), None);
    assert_eq!(fitted_suffix(70.0, None), None);
}
