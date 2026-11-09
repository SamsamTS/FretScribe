//! Grace notes (`Note.grace_of`) and forced hammer-ons / pull-offs, both added by hand.
//!
//! A grace note takes no rhythmic time: it sits just before its main note (several graces of one
//! main note chain backwards, the last added being the earliest).

use std::collections::HashSet;

use thiserror::Error;

use crate::links::{legato_kind, strings_differ, LinkError, MAX_FRET};
use crate::model::{GraceKind, GraceNote, Legato, Note, Project};
use crate::tempo::{NoteTimes, TempoMap};
use crate::tracks::next_note_id;

pub const ACCIACCATURA_SEC: f64 = 0.04;
pub const APPOGGIATURA_SEC: f64 = 0.1;
const MIN_GRACE_SEC: f64 = 1e-3;
const SAME_SEC: f64 = 1e-9;

#[derive(Clone, Debug, Error, PartialEq)]
pub enum GraceError {
    #[error("note {0} does not exist")]
    UnknownNote(u64),
    #[error("a grace note cannot have a grace note")]
    MainIsGrace,
    #[error("note {0} is not a grace note")]
    NotGrace(u64),
    #[error("there is no room before the note")]
    NoRoom,
    #[error("the pitch is outside the MIDI range")]
    Pitch,
}

#[derive(Clone, Debug, Error, PartialEq)]
pub enum LegatoError {
    #[error(transparent)]
    Link(#[from] LinkError),
    #[error("the pitch direction does not match a {0:?}")]
    KindMismatch(Legato),
    /// The notes are on different strings; `candidates` are the destination's placements on the
    /// source's string (empty never happens: that is `LinkError::NoSameString`).
    #[error("the notes are on different strings")]
    NeedsSameString { candidates: Vec<(u8, u8)> },
}

impl From<LegatoError> for LinkError {
    fn from(e: LegatoError) -> Self {
        match e {
            LegatoError::Link(e) => e,
            _ => LinkError::NoSameString,
        }
    }
}

pub fn grace_default_sec(kind: GraceKind) -> f64 {
    match kind {
        GraceKind::Acciaccatura => ACCIACCATURA_SEC,
        GraceKind::Appoggiatura => APPOGGIATURA_SEC,
    }
}

/// A whole tone below the main note.
pub fn default_grace_midi(main_midi: u8) -> u8 {
    main_midi.saturating_sub(2)
}

impl Project {
    fn grace_siblings(&self, main: u64) -> Vec<u64> {
        self.notes.iter().filter(|n| n.grace_of.is_some_and(|g| g.main == main)).map(|n| n.id).collect()
    }

    /// Start (seconds) of the latest note before `limit` that is not part of the main note's group.
    fn previous_start(&self, times: &NoteTimes, main: u64, limit: f64) -> f64 {
        let group = self.grace_siblings(main);
        self.notes
            .iter()
            .filter(|n| n.id != main && !group.contains(&n.id))
            .filter_map(|n| times.start_sec(n.id))
            .filter(|&s| s < limit - SAME_SEC)
            .fold(f64::NEG_INFINITY, f64::max)
    }

    /// Creates a grace note ending where the earliest existing grace of `main` starts (or at the
    /// main note). Its length is the kind's default, at most half the distance to the start of the
    /// previous note.
    pub fn add_grace_note(&mut self, main: u64, midi: Option<u8>, kind: GraceKind) -> Result<u64, GraceError> {
        let main_note = self.note(main).ok_or(GraceError::UnknownNote(main))?;
        if main_note.grace_of.is_some() {
            return Err(GraceError::MainIsGrace);
        }
        let midi = midi.unwrap_or_else(|| default_grace_midi(main_note.midi));
        if midi > 127 {
            return Err(GraceError::Pitch);
        }
        let tempo = TempoMap::new(&self.measures);
        let times = NoteTimes::build(&self.notes, &tempo);
        let end_pos = self
            .notes
            .iter()
            .filter(|n| n.grace_of.is_some_and(|g| g.main == main))
            .min_by(|a, b| times.start_sec(a.id).partial_cmp(&times.start_sec(b.id)).unwrap_or(std::cmp::Ordering::Equal))
            .map_or(main_note.start, |n| n.start);
        let end = tempo.pos_to_sec(end_pos);
        let len = grace_default_sec(kind).min((end - self.previous_start(&times, main, end)) / 2.0);
        if len < MIN_GRACE_SEC {
            return Err(GraceError::NoRoom);
        }
        let id = next_note_id(self);
        let mut note = Note::new(id, midi, tempo.sec_to_pos(end - len), end_pos);
        note.grace_of = Some(GraceNote { main, kind });
        self.notes.push(note);
        Ok(id)
    }

    fn grace_mut(&mut self, id: u64) -> Result<&mut Note, GraceError> {
        let note = self.notes.iter_mut().find(|n| n.id == id).ok_or(GraceError::UnknownNote(id))?;
        if note.grace_of.is_none() {
            return Err(GraceError::NotGrace(id));
        }
        Ok(note)
    }

    pub fn set_grace_kind(&mut self, id: u64, kind: GraceKind) -> Result<(), GraceError> {
        if let Some(g) = &mut self.grace_mut(id)?.grace_of {
            g.kind = kind;
        }
        Ok(())
    }

    /// Moves a grace note (same length) to start at `start_sec`, kept inside the gap between the
    /// start of the previous note and its main note.
    pub fn move_grace(&mut self, id: u64, start_sec: f64) -> Result<(), GraceError> {
        let main = self.grace_mut(id)?.grace_of.map_or(0, |g| g.main);
        let tempo = TempoMap::new(&self.measures);
        let times = NoteTimes::build(&self.notes, &tempo);
        let (Some(main_start), Some(start), Some(end)) = (self.note(main).and_then(|m| times.start_sec(m.id)), times.start_sec(id), times.end_sec(id)) else {
            return Err(GraceError::UnknownNote(main));
        };
        let (lowest, highest) = (self.previous_start(&times, main, main_start) + MIN_GRACE_SEC, main_start - (end - start));
        if highest < lowest {
            return Err(GraceError::NoRoom);
        }
        let new_start = start_sec.clamp(lowest, highest);
        let (s, e) = (tempo.sec_to_pos(new_start), tempo.sec_to_pos(new_start + (end - start)));
        let note = self.grace_mut(id)?;
        (note.start, note.end) = (s, e);
        Ok(())
    }

    /// Removes a grace note and the legato links that point at it or start from it.
    pub fn remove_grace(&mut self, id: u64) -> Result<(), GraceError> {
        self.grace_mut(id)?;
        self.remove_note_set(&HashSet::from([id]));
        Ok(())
    }

    /// Hammer-on or pull-off from `from` to `to`; `kind = None` infers it from the pitch direction.
    /// Notes assigned to different strings give `NeedsSameString` with the destination's placements
    /// on the source's string.
    pub fn link_legato(&mut self, from: u64, to: u64, kind: Option<Legato>) -> Result<Legato, LegatoError> {
        self.can_link(from, to, crate::links::LinkType::Legato)?;
        let (a, b) = (self.note(from).ok_or(LinkError::UnknownNote(from))?, self.note(to).ok_or(LinkError::UnknownNote(to))?);
        let inferred = legato_kind(a, b);
        if kind.is_some_and(|k| k != inferred) {
            return Err(LegatoError::KindMismatch(kind.unwrap_or(inferred)));
        }
        if strings_differ(a, b) {
            let open = a.string_fret.and_then(|(s, _)| self.tuning.open_strings.get(usize::from(s)).map(|&o| (s, o)));
            let candidates: Vec<(u8, u8)> = open
                .and_then(|(s, o)| b.midi.checked_sub(o).filter(|f| (self.capo..=MAX_FRET).contains(f)).map(|f| (s, f)))
                .into_iter()
                .collect();
            return Err(if candidates.is_empty() { LinkError::NoSameString.into() } else { LegatoError::NeedsSameString { candidates } });
        }
        self.write_link(from, to, crate::links::LinkType::Legato);
        Ok(inferred)
    }

    /// Like `link_legato(from, to, None)`, but when the notes are on different strings the
    /// destination moves to its lowest-fret placement on the source's string and is locked there.
    pub fn force_legato(&mut self, from: u64, to: u64) -> Result<Legato, LegatoError> {
        match self.link_legato(from, to, None) {
            Err(LegatoError::NeedsSameString { candidates }) => {
                let placement = candidates.into_iter().min_by_key(|c| c.1);
                if let Some(note) = self.notes.iter_mut().find(|n| n.id == to) {
                    note.string_fret = placement;
                    note.locked_fret = true;
                }
                self.link_legato(from, to, None)
            }
            other => other,
        }
    }

    /// Removes the legato link that ends at `to`.
    pub fn unlink_legato(&mut self, to: u64) -> bool {
        self.notes.iter_mut().find(|n| n.id == to).is_some_and(|n| n.legato.take().is_some())
    }
}

#[cfg(test)]
mod tests;
