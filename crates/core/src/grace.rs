//! Grace notes (`Note.grace_of`) and forced hammer-ons / pull-offs, both added by hand.
//!
//! A grace note takes no rhythmic time. Its kind is its placement: an Acciaccatura is before the
//! beat (its end is where the note it leads into starts, several of them chain backwards, the last
//! added being the earliest), an Appoggiatura is on the beat (its start is the start of the main
//! note, over which it is drawn).

use std::collections::HashSet;

use thiserror::Error;

use crate::links::{legato_kind, strings_differ, LinkError, LinkType, MAX_FRET};
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
    #[error("there is no room for the grace note")]
    NoRoom,
    #[error("the pitch is outside the MIDI range")]
    Pitch,
    #[error("that link is not possible here")]
    LinkInvalid,
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

/// Link kinds tried, in this order, when a grace note gets its first link.
const DEFAULT_LINKS: [LinkType; 3] = [LinkType::Legato, LinkType::Slide, LinkType::Dashed];

/// The note a grace note leads into: the earliest group member (its main note or a sibling grace
/// note) starting after it.
pub fn grace_target_of(notes: &[Note], times: &NoteTimes, id: u64) -> Option<u64> {
    let main = notes.iter().find(|n| n.id == id)?.grace_of?.main;
    let start = times.start_sec(id)?;
    notes
        .iter()
        .filter(|n| n.id == main || n.grace_of.is_some_and(|g| g.main == main))
        .filter_map(|n| Some((n.id, times.start_sec(n.id)?)))
        .filter(|&(other, s)| other != id && s > start - SAME_SEC)
        .min_by(|a, b| a.1.total_cmp(&b.1))
        .map(|(other, _)| other)
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

    /// Where a grace note of `kind` is anchored (seconds) and how much room it has. Before the beat
    /// the anchor is the start of the nearest later grace note of the group, else of the main note,
    /// and the room runs back to the start of the previous note; on the beat the anchor is the start
    /// of the main note and the room is its length. `after` ignores the grace notes starting before it.
    fn grace_room(&self, times: &NoteTimes, main: u64, id: Option<u64>, kind: GraceKind, after: f64) -> Option<(f64, f64)> {
        let main_start = times.start_sec(main)?;
        if kind == GraceKind::Appoggiatura {
            return Some((main_start, times.end_sec(main)? - main_start));
        }
        let end = self
            .notes
            .iter()
            .filter(|n| Some(n.id) != id && n.grace_of.is_some_and(|g| g.main == main && g.kind == GraceKind::Acciaccatura))
            .filter_map(|n| times.start_sec(n.id))
            .filter(|&s| s > after + SAME_SEC)
            .fold(main_start, f64::min);
        Some((end, end - self.previous_start(times, main, end)))
    }

    /// Puts grace note `id` in place as `kind` with a length of at most `len_sec`, then re-checks
    /// its link.
    fn place_grace(&mut self, id: u64, kind: GraceKind, len_sec: f64, after: f64) -> Result<(), GraceError> {
        let main = self.grace_mut(id)?.grace_of.map_or(0, |g| g.main);
        let tempo = TempoMap::new(&self.measures);
        let times = NoteTimes::build(&self.notes, &tempo);
        let (anchor, room) = self.grace_room(&times, main, Some(id), kind, after).ok_or(GraceError::UnknownNote(main))?;
        let len = len_sec.min(if kind == GraceKind::Acciaccatura { room - MIN_GRACE_SEC } else { room });
        if len < MIN_GRACE_SEC {
            return Err(GraceError::NoRoom);
        }
        let (start, end) = if kind == GraceKind::Acciaccatura { (anchor - len, anchor) } else { (anchor, anchor + len) };
        let (start, end) = (tempo.sec_to_pos(start), tempo.sec_to_pos(end));
        let note = self.grace_mut(id)?;
        (note.start, note.end) = (start, end);
        if let Some(g) = &mut note.grace_of {
            g.kind = kind;
        }
        self.refresh_grace_link(id);
        self.resolve_overlaps(&[id]);
        Ok(())
    }

    /// Creates a grace note with the kind's default length (at most half the room): before the beat
    /// it ends where the earliest existing grace note of `main` starts (or at the main note), on the
    /// beat it starts with the main note.
    pub fn add_grace_note(&mut self, main: u64, midi: Option<u8>, kind: GraceKind) -> Result<u64, GraceError> {
        let main_note = self.note(main).ok_or(GraceError::UnknownNote(main))?;
        if main_note.grace_of.is_some() {
            return Err(GraceError::MainIsGrace);
        }
        let midi = midi.unwrap_or_else(|| default_grace_midi(main_note.midi));
        if midi > 127 {
            return Err(GraceError::Pitch);
        }
        let id = next_note_id(self);
        let mut note = Note::new(id, midi, main_note.start, main_note.end);
        note.grace_of = Some(GraceNote { main, kind });
        self.notes.push(note);
        let times = NoteTimes::build(&self.notes, &TempoMap::new(&self.measures));
        let room = self.grace_room(&times, main, Some(id), kind, f64::NEG_INFINITY).map_or(0.0, |r| r.1);
        let placed = self.place_grace(id, kind, grace_default_sec(kind).min(room / 2.0), f64::NEG_INFINITY);
        if let Err(e) = placed {
            self.remove_note_set(&HashSet::from([id]));
            return Err(e);
        }
        let link = self.default_grace_link(id);
        let _ = self.set_grace_link(id, link);
        Ok(id)
    }

    /// Re-times a grace note: `kind` decides the side of the beat, `len_sec` its length (limited by
    /// the room).
    pub fn set_grace_timing(&mut self, id: u64, kind: GraceKind, len_sec: f64) -> Result<(), GraceError> {
        let after = NoteTimes::build(&self.notes, &TempoMap::new(&self.measures)).start_sec(id).unwrap_or(f64::NEG_INFINITY);
        self.place_grace(id, kind, len_sec, after)
    }

    /// Switches between before the beat and on the beat, keeping the length.
    pub fn set_grace_kind(&mut self, id: u64, kind: GraceKind) -> Result<(), GraceError> {
        let times = NoteTimes::build(&self.notes, &TempoMap::new(&self.measures));
        let len = times.end_sec(id).zip(times.start_sec(id)).map_or(grace_default_sec(kind), |(e, s)| e - s);
        self.grace_mut(id)?;
        self.set_grace_timing(id, kind, len)
    }

    pub fn grace_target(&self, id: u64) -> Option<u64> {
        grace_target_of(&self.notes, &NoteTimes::build(&self.notes, &TempoMap::new(&self.measures)), id)
    }

    pub fn grace_link(&self, id: u64) -> LinkType {
        self.link_of(id).map_or(LinkType::Dashed, |l| l.1)
    }

    /// Replaces the grace note's link; refused when `link` is not valid.
    pub fn set_grace_link(&mut self, id: u64, link: LinkType) -> Result<(), GraceError> {
        self.grace_mut(id)?;
        if !self.valid_links(id).contains(&link) {
            return Err(GraceError::LinkInvalid);
        }
        self.set_link(id, link);
        Ok(())
    }

    /// Legato when possible, else slide, else dashed.
    pub fn default_grace_link(&self, id: u64) -> LinkType {
        let valid = self.valid_links(id);
        DEFAULT_LINKS.into_iter().find(|k| valid.contains(k)).unwrap_or(LinkType::Dashed)
    }

    /// Keeps the current link while it stays valid and still leads into the right note, otherwise
    /// takes the default.
    fn refresh_grace_link(&mut self, id: u64) {
        let stale = self.outgoing(id).is_some_and(|(to, _)| Some(to) != self.grace_target(id));
        if stale {
            self.unlink(id);
        }
        if stale || !self.valid_links(id).contains(&self.grace_link(id)) {
            let _ = self.set_grace_link(id, self.default_grace_link(id));
        }
    }

    fn grace_mut(&mut self, id: u64) -> Result<&mut Note, GraceError> {
        let note = self.notes.iter_mut().find(|n| n.id == id).ok_or(GraceError::UnknownNote(id))?;
        if note.grace_of.is_none() {
            return Err(GraceError::NotGrace(id));
        }
        Ok(note)
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
