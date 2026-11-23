//! Typed exact values of one note: start, end, pitch and an optional string lock.

use fretscribe_core::{midi_to_name, parse_pitch, EditError, Project, ProjectEdit, TimePos, MAX_FRET};
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
    (bar.to_string(), format!("{:.4}", pos.beat() + 1.0).trim_end_matches('0').trim_end_matches('.').to_owned())
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

fn position(project: &Project, (bar, beat): &(String, String)) -> Result<TimePos, ExactError> {
    let index = bar.trim().parse::<usize>().ok().filter(|b| *b >= 1).ok_or_else(|| ExactError::Bar(bar.clone()))?;
    let measure = project.measures.get(index - 1).ok_or_else(|| ExactError::Bar(bar.clone()))?;
    let beat = parse_number(beat).filter(|b| *b >= 1.0).ok_or_else(|| ExactError::Beat(beat.clone()))?;
    Ok(TimePos::at(measure.id, beat - 1.0))
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

#[cfg(all(test, dev_tests))]
#[path = "../../../../dev/tests/unit/ui/note_editor/exact.rs"]
mod tests;
