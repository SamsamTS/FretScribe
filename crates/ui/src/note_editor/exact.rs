//! Typed exact values of one note: start, end, pitch and an optional string lock.

use fretscribe_core::{midi_to_name, EditError, Project, ProjectEdit, TimePos, MAX_FRET};
use thiserror::Error;

#[derive(Clone, Debug, PartialEq)]
pub struct ExactFields {
    pub id: u64,
    /// 1-based bar and 1-based beat (fractions such as `1/3` allowed), like a musician reads them.
    pub start: (String, String),
    pub end: (String, String),
    pub pitch: String,
    pub string: Option<u8>,
}

#[derive(Debug, Error, PartialEq)]
pub enum ExactError {
    #[error("unknown note")]
    UnknownNote,
    #[error("bar {0} does not exist")]
    Bar(String),
    #[error("cannot read the beat '{0}'")]
    Beat(String),
    #[error("cannot read the pitch '{0}'")]
    Pitch(String),
    #[error("the end must come after the start")]
    Order,
    #[error("that string cannot play this pitch")]
    String,
    #[error(transparent)]
    Edit(#[from] EditError),
}

fn bar_beat(project: &Project, pos: TimePos) -> (String, String) {
    let tempo = fretscribe_core::TempoMap::new(&project.measures);
    let pos = tempo.normalise(pos);
    let bar = project.measures.iter().position(|m| m.id == pos.measure).map_or(1, |i| i + 1);
    (bar.to_string(), format!("{:.4}", pos.beat + 1.0).trim_end_matches('0').trim_end_matches('.').to_owned())
}

pub fn fields_for(project: &Project, id: u64) -> Option<ExactFields> {
    let n = project.note(id)?;
    Some(ExactFields { id, start: bar_beat(project, n.start), end: bar_beat(project, n.end), pitch: midi_to_name(n.midi), string: n.locked_fret.then_some(n.string_fret).flatten().map(|(s, _)| s) })
}

fn parse_number(text: &str) -> Option<f64> {
    let text = text.trim();
    match text.split_once('/') {
        Some((a, b)) => {
            let (a, b) = (a.trim().parse::<f64>().ok()?, b.trim().parse::<f64>().ok()?);
            (b != 0.0).then_some(a / b)
        }
        None => text.parse().ok(),
    }
}

/// A MIDI number or a name such as `E2`, `F#3`, `Bb4`.
pub fn parse_pitch(text: &str) -> Option<u8> {
    let text = text.trim();
    if let Ok(n) = text.parse::<u8>() {
        return (n <= 127).then_some(n);
    }
    let mut chars = text.chars();
    let base = match chars.next()?.to_ascii_uppercase() {
        'C' => 0,
        'D' => 2,
        'E' => 4,
        'F' => 5,
        'G' => 7,
        'A' => 9,
        'B' => 11,
        _ => return None,
    };
    let rest = chars.as_str();
    let (shift, octave) = match rest.chars().next()? {
        '#' => (1, &rest[1..]),
        'b' => (-1, &rest[1..]),
        _ => (0, rest),
    };
    let midi = (octave.parse::<i32>().ok()? + 1) * 12 + base + shift;
    u8::try_from(midi).ok().filter(|m| *m <= 127)
}

fn position(project: &Project, (bar, beat): &(String, String)) -> Result<TimePos, ExactError> {
    let index = bar.trim().parse::<usize>().ok().filter(|b| *b >= 1).ok_or_else(|| ExactError::Bar(bar.clone()))?;
    let measure = project.measures.get(index - 1).ok_or_else(|| ExactError::Bar(bar.clone()))?;
    let beat = parse_number(beat).filter(|b| *b >= 1.0).ok_or_else(|| ExactError::Beat(beat.clone()))?;
    Ok(TimePos::new(measure.id, beat - 1.0))
}

pub fn apply_exact(project: &mut Project, fields: &ExactFields) -> Result<ProjectEdit, ExactError> {
    project.note(fields.id).ok_or(ExactError::UnknownNote)?;
    let (start, end) = (position(project, &fields.start)?, position(project, &fields.end)?);
    let tempo = fretscribe_core::TempoMap::new(&project.measures);
    if tempo.pos_to_beats(end) <= tempo.pos_to_beats(start) {
        return Err(ExactError::Order);
    }
    let midi = parse_pitch(&fields.pitch).ok_or_else(|| ExactError::Pitch(fields.pitch.clone()))?;
    let lock = match fields.string {
        Some(s) => {
            let fret = project.tuning.open_strings.get(usize::from(s)).and_then(|open| midi.checked_sub(*open)).filter(|f| (project.capo..=MAX_FRET).contains(f)).ok_or(ExactError::String)?;
            Some((s, fret))
        }
        None => None,
    };
    Ok(project.edit_notes("Edit note", |p| {
        if let Some(n) = p.notes.iter_mut().find(|n| n.id == fields.id) {
            (n.start, n.end, n.midi) = (start, end, midi);
            n.string_fret = lock;
            n.locked_fret = lock.is_some();
        }
        p.resolve_overlaps(&[fields.id]);
        Ok(())
    })?)
}

#[cfg(test)]
mod tests {
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
        assert!((n.start.beat - 1.0 / 3.0).abs() < 1e-9);
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
}
