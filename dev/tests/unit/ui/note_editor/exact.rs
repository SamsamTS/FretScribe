use super::super::testkit::*;
use super::*;

#[test]
fn fields_are_read_like_a_musician_and_round_trip() {
    let mut p = project();
    let id = add(&mut p, 64, 1.0, 2.5);
    let f = fields_for(&p, id).unwrap();
    assert_eq!((f.start.clone(), f.end.clone(), f.pitch.as_str()), (("1".into(), "2".into()), ("1".into(), "3.5".into()), "E4"));
    let before = p.notes.clone();
    let edit = apply_exact(&mut p, &f).unwrap();
    assert_eq!(p.notes, before);
    edit.undo(&mut p);
}

#[test]
fn typed_values_replace_the_note_and_can_lock_a_string() {
    let mut p = project();
    let id = add(&mut p, 64, 1.0, 2.0);
    let mut f = fields_for(&p, id).unwrap();
    f.start = ("2".into(), "4/3".into());
    f.end = ("2".into(), "2".into());
    f.pitch = "F#3".into();
    f.string = Some(2);
    let edit = apply_exact(&mut p, &f).unwrap();
    let n = p.note(id).unwrap();
    assert_eq!((n.start.measure, n.midi, n.string_fret, n.locked_fret), (p.measures[1].id, 54, Some((2, 4)), true));
    assert!((n.start.beat() - 1.0 / 3.0).abs() < 1e-9);
    edit.undo(&mut p);
    assert_eq!(p.note(id).unwrap().midi, 64);
}

#[test]
fn bad_input_is_refused_without_touching_the_project() {
    let mut p = project();
    let id = add(&mut p, 64, 1.0, 2.0);
    let base = fields_for(&p, id).unwrap();
    let try_with = |p: &mut Project, f: ExactFields| apply_exact(p, &f).unwrap_err();
    assert_eq!(try_with(&mut p, ExactFields { start: ("99".into(), "1".into()), ..base.clone() }), ExactError::Bar("99".into()));
    assert_eq!(try_with(&mut p, ExactFields { start: ("1".into(), "0".into()), ..base.clone() }), ExactError::Beat("0".into()));
    assert_eq!(try_with(&mut p, ExactFields { pitch: "H9".into(), ..base.clone() }), ExactError::Pitch("H9".into()));
    assert_eq!(try_with(&mut p, ExactFields { end: base.start.clone(), ..base.clone() }), ExactError::Order);
    assert_eq!(try_with(&mut p, ExactFields { string: Some(0), pitch: "C1".into(), ..base }), ExactError::String);
    assert_eq!(p.note(id).unwrap().midi, 64);
}

#[test]
fn pitch_names_and_numbers() {
    assert_eq!([parse_pitch("E2"), parse_pitch("c4"), parse_pitch("Bb3"), parse_pitch("60"), parse_pitch("A#2")], [Some(40), Some(60), Some(58), Some(60), Some(46)]);
    assert_eq!([parse_pitch("200"), parse_pitch(""), parse_pitch("X1")], [None, None, None]);
}
