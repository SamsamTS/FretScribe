//! Links between notes: slide and bend live on the start note (`link_to` + `technique`), legato on
//! the end note (`legato.from`). A chain is a simple linked list in time order.

use std::collections::HashSet;

use thiserror::Error;

use crate::edit::ProjectEdit;
use crate::model::{Legato, LegatoLink, Note, Project, Technique, Tuning};
use crate::tempo::{NoteTimes, TempoMap};

pub const MAX_FRET: u8 = 24;
const BEND_MAX_SEMITONES: u8 = 4;
const LEGATO_MAX_GAP_BEATS: f64 = 0.25;
const SAME_SEC: f64 = 1e-9;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LinkType {
    Slide,
    Bend,
    Legato,
}

impl LinkType {
    const CYCLE: [Self; 3] = [Self::Slide, Self::Bend, Self::Legato];
}

#[derive(Clone, Copy, Debug, Error, PartialEq, Eq)]
pub enum LinkError {
    #[error("note {0} does not exist")]
    UnknownNote(u64),
    #[error("a note cannot link to itself")]
    SameNote,
    #[error("the end note must not start before the start note")]
    NotInOrder,
    #[error("note {0} already starts a link")]
    HasOutgoing(u64),
    #[error("note {0} already ends a link")]
    HasIncoming(u64),
    #[error("the link would form a cycle")]
    Cycle,
    #[error("both notes have the same pitch")]
    SamePitch,
    #[error("a bend spans 1 to 4 semitones")]
    BendInterval,
    #[error("no placement on one string")]
    NoSameString,
    #[error("the notes are not adjacent")]
    NotAdjacent,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct LinkReport {
    pub linked: usize,
    pub skipped: Vec<(u64, u64, LinkError)>,
}

/// Legato when a same-string placement exists, slide otherwise.
pub fn infer_link(tuning: &Tuning, capo: u8, from_midi: u8, to_midi: u8) -> LinkType {
    if from_midi != to_midi && same_string_fit(tuning, capo, &[from_midi, to_midi]) {
        LinkType::Legato
    } else {
        LinkType::Slide
    }
}

/// Whether one string holds every pitch between the capo and `MAX_FRET`.
pub fn same_string_fit(tuning: &Tuning, capo: u8, midis: &[u8]) -> bool {
    tuning.open_strings.iter().any(|&open| {
        midis
            .iter()
            .all(|&m| m.checked_sub(open).is_some_and(|fret| (capo..=MAX_FRET).contains(&fret)))
    })
}

fn legato_kind(from: &Note, to: &Note) -> Legato {
    if to.midi > from.midi {
        Legato::HammerOn
    } else {
        Legato::PullOff
    }
}

impl Project {
    pub fn outgoing(&self, id: u64) -> Option<(u64, LinkType)> {
        let note = self.note(id)?;
        if let Some(to) = note.link_to {
            let kind = if note.technique == Some(Technique::Bend) { LinkType::Bend } else { LinkType::Slide };
            return Some((to, kind));
        }
        self.notes
            .iter()
            .find(|n| n.legato.is_some_and(|l| l.from == id))
            .map(|n| (n.id, LinkType::Legato))
    }

    pub fn incoming(&self, id: u64) -> Option<(u64, LinkType)> {
        if let Some(l) = self.note(id)?.legato {
            return Some((l.from, LinkType::Legato));
        }
        self.notes.iter().find(|n| n.link_to == Some(id)).map(|n| {
            let kind = if n.technique == Some(Technique::Bend) { LinkType::Bend } else { LinkType::Slide };
            (n.id, kind)
        })
    }

    /// The whole chain `id` belongs to, first note first.
    pub fn chain_of(&self, id: u64) -> Vec<u64> {
        if self.note(id).is_none() {
            return Vec::new();
        }
        let mut head = id;
        let mut seen = HashSet::from([id]);
        while let Some((prev, _)) = self.incoming(head).filter(|(p, _)| seen.insert(*p)) {
            head = prev;
        }
        let mut chain = vec![head];
        let mut seen = HashSet::from([head]);
        while let Some((next, _)) = self.outgoing(*chain.last().unwrap_or(&head)).filter(|(n, _)| seen.insert(*n)) {
            chain.push(next);
        }
        chain
    }

    pub fn chain_fits_one_string(&self, id: u64) -> bool {
        let midis: Vec<u8> = self.chain_of(id).iter().filter_map(|&n| self.note(n)).map(|n| n.midi).collect();
        same_string_fit(&self.tuning, self.capo, &midis)
    }

    pub fn infer_link_type(&self, from: u64, to: u64) -> LinkType {
        match (self.note(from), self.note(to)) {
            (Some(a), Some(b)) => infer_link(&self.tuning, self.capo, a.midi, b.midi),
            _ => LinkType::Slide,
        }
    }

    pub fn can_link(&self, from: u64, to: u64, kind: LinkType) -> Result<(), LinkError> {
        let tempo = TempoMap::new(&self.measures);
        self.check_link(from, to, kind, &NoteTimes::build(&self.notes, &tempo), &tempo)
    }

    fn check_link(&self, from: u64, to: u64, kind: LinkType, times: &NoteTimes, tempo: &TempoMap) -> Result<(), LinkError> {
        let (a, b) = (self.note(from).ok_or(LinkError::UnknownNote(from))?, self.note(to).ok_or(LinkError::UnknownNote(to))?);
        if from == to {
            return Err(LinkError::SameNote);
        }
        let (sa, sb) = (times.start_sec(from).unwrap_or(0.0), times.start_sec(to).unwrap_or(0.0));
        if sb < sa - SAME_SEC {
            return Err(LinkError::NotInOrder);
        }
        if self.outgoing(from).is_some() {
            return Err(LinkError::HasOutgoing(from));
        }
        if self.incoming(to).is_some() {
            return Err(LinkError::HasIncoming(to));
        }
        if self.chain_of(to).contains(&from) {
            return Err(LinkError::Cycle);
        }
        let interval = a.midi.abs_diff(b.midi);
        if interval == 0 {
            return Err(LinkError::SamePitch);
        }
        match kind {
            LinkType::Slide => {}
            LinkType::Bend if !(1..=BEND_MAX_SEMITONES).contains(&interval) => return Err(LinkError::BendInterval),
            LinkType::Bend => {}
            LinkType::Legato => {
                if !same_string_fit(&self.tuning, self.capo, &[a.midi, b.midi]) {
                    return Err(LinkError::NoSameString);
                }
                let gap = tempo.pos_to_beats(b.start).zip(tempo.pos_to_beats(a.end)).map(|(s, e)| (s - e).abs());
                if gap.is_some_and(|g| g > LEGATO_MAX_GAP_BEATS) {
                    return Err(LinkError::NotAdjacent);
                }
            }
        }
        Ok(())
    }

    fn write_link(&mut self, from: u64, to: u64, kind: LinkType) {
        let Some(legato) = self.note(from).zip(self.note(to)).map(|(a, b)| legato_kind(a, b)) else {
            return;
        };
        for n in &mut self.notes {
            if n.id == from && kind != LinkType::Legato {
                n.link_to = Some(to);
                n.technique = Some(if kind == LinkType::Bend { Technique::Bend } else { Technique::Slide });
            } else if n.id == to && kind == LinkType::Legato {
                n.legato = Some(LegatoLink { kind: legato, from });
            }
        }
    }

    pub fn link_notes(&mut self, from: u64, to: u64, kind: LinkType) -> Result<(), LinkError> {
        let tempo = TempoMap::new(&self.measures);
        let times = NoteTimes::build(&self.notes, &tempo);
        self.check_link(from, to, kind, &times, &tempo)?;
        self.write_link(from, to, kind);
        Ok(())
    }

    /// Removes the link that starts at `from`; the notes stay.
    pub fn unlink(&mut self, from: u64) -> bool {
        let Some((to, kind)) = self.outgoing(from) else {
            return false;
        };
        for n in &mut self.notes {
            if n.id == from && kind != LinkType::Legato {
                n.link_to = None;
                n.technique = None;
            } else if n.id == to && kind == LinkType::Legato {
                n.legato = None;
            }
        }
        true
    }

    /// Cuts the link that ends at `id`, so `id` starts a new chain.
    pub fn split_chain(&mut self, id: u64) -> bool {
        self.incoming(id).is_some_and(|(prev, _)| self.unlink(prev))
    }

    /// Next valid type of the outgoing link of `id` in the order slide, bend, legato; `None` when
    /// there is no link or no other type is valid.
    pub fn cycle_link_type(&mut self, id: u64) -> Option<LinkType> {
        let (to, current) = self.outgoing(id)?;
        let tempo = TempoMap::new(&self.measures);
        let times = NoteTimes::build(&self.notes, &tempo);
        let at = LinkType::CYCLE.iter().position(|&k| k == current)?;
        self.unlink(id);
        let next = (1..LinkType::CYCLE.len())
            .map(|step| LinkType::CYCLE[(at + step) % LinkType::CYCLE.len()])
            .find(|&k| self.check_link(id, to, k, &times, &tempo).is_ok());
        self.write_link(id, to, next.unwrap_or(current));
        next
    }

    /// Links consecutive groups of overlapping notes pair by pair in pitch order, inferring the type.
    pub fn link_selection(&mut self, ids: &[u64]) -> LinkReport {
        let tempo = TempoMap::new(&self.measures);
        let times = NoteTimes::build(&self.notes, &tempo);
        let mut notes: Vec<&Note> = ids.iter().filter_map(|&id| self.note(id)).collect();
        notes.sort_by(|a, b| {
            let start = |n: &Note| times.start_sec(n.id).unwrap_or(0.0);
            start(a).total_cmp(&start(b)).then(a.midi.cmp(&b.midi))
        });
        let mut groups: Vec<Vec<u64>> = Vec::new();
        let mut group_end = f64::NEG_INFINITY;
        for n in notes {
            let (start, end) = (times.start_sec(n.id).unwrap_or(0.0), times.end_sec(n.id).unwrap_or(0.0));
            if groups.is_empty() || start >= group_end - SAME_SEC {
                groups.push(Vec::new());
                group_end = end;
            }
            group_end = group_end.max(end);
            if let Some(group) = groups.last_mut() {
                group.push(n.id);
            }
        }
        let by_pitch = |group: &mut Vec<u64>, p: &Project| group.sort_by_key(|&id| p.note(id).map_or(0, |n| n.midi));
        for group in &mut groups {
            by_pitch(group, self);
        }
        let mut report = LinkReport::default();
        for pair in groups.windows(2) {
            for (&from, &to) in pair[0].iter().zip(&pair[1]) {
                let kind = self.infer_link_type(from, to);
                let result = self.check_link(from, to, kind, &times, &tempo);
                match result {
                    Ok(()) => {
                        self.write_link(from, to, kind);
                        report.linked += 1;
                    }
                    Err(e) => report.skipped.push((from, to, e)),
                }
            }
        }
        report
    }

    /// Deleting a middle note splits its chain, or reconnects the neighbours when `bridge`.
    pub fn delete_notes(&mut self, ids: &[u64], bridge: bool) -> ProjectEdit {
        self.transact_infallible("Delete notes", |p| {
            let doomed = p.with_grace_notes(ids);
            let bridges: Vec<(u64, u64, LinkType)> = if bridge {
                p.notes
                    .iter()
                    .filter(|n| !doomed.contains(&n.id))
                    .filter_map(|n| {
                        let (first, kind) = p.outgoing(n.id).filter(|(to, _)| doomed.contains(to))?;
                        let mut next = first;
                        while let Some((after, _)) = p.outgoing(next).filter(|_| doomed.contains(&next)) {
                            next = after;
                        }
                        (!doomed.contains(&next)).then_some((n.id, next, kind))
                    })
                    .collect()
            } else {
                Vec::new()
            };
            p.remove_note_set(&doomed);
            for (from, to, kind) in bridges {
                if p.link_notes(from, to, kind).is_err() {
                    let inferred = p.infer_link_type(from, to);
                    let _ = p.link_notes(from, to, inferred);
                }
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{TimePos, TimeSignature, Track, TrackRole};
    use crate::tracks::next_note_id;

    fn project() -> Project {
        Project::new(Track::new(1, "Guitar", TrackRole::Guitar), 8.0, 120.0, TimeSignature::COMMON)
    }

    fn add(p: &mut Project, midi: u8, start: f64, end: f64) -> u64 {
        let id = next_note_id(p);
        p.notes.push(Note::new(id, midi, TimePos::new(1, start), TimePos::new(1, end)));
        id
    }

    fn chain3(p: &mut Project) -> [u64; 3] {
        let ids = [add(p, 60, 0.0, 1.0), add(p, 62, 1.0, 2.0), add(p, 64, 2.0, 3.0)];
        p.link_notes(ids[0], ids[1], LinkType::Slide).unwrap();
        p.link_notes(ids[1], ids[2], LinkType::Slide).unwrap();
        ids
    }

    #[test]
    fn link_stores_slide_and_bend_on_the_start_note_and_legato_on_the_end_note() {
        let mut p = project();
        let (a, b, c, d) = (add(&mut p, 60, 0.0, 1.0), add(&mut p, 62, 1.0, 2.0), add(&mut p, 64, 2.0, 3.0), add(&mut p, 62, 3.0, 4.0));
        p.link_notes(a, b, LinkType::Bend).unwrap();
        assert_eq!((p.note(a).unwrap().link_to, p.note(a).unwrap().technique), (Some(b), Some(Technique::Bend)));
        p.link_notes(b, c, LinkType::Legato).unwrap();
        assert_eq!(p.note(c).unwrap().legato, Some(LegatoLink { kind: Legato::HammerOn, from: b }));
        assert_eq!(p.note(b).unwrap().link_to, None);
        p.link_notes(c, d, LinkType::Legato).unwrap();
        assert_eq!(p.note(d).unwrap().legato.map(|l| l.kind), Some(Legato::PullOff));
        assert_eq!(p.outgoing(b), Some((c, LinkType::Legato)));
        assert_eq!(p.incoming(b), Some((a, LinkType::Bend)));
        assert_eq!(p.chain_of(c), [a, b, c, d]);
    }

    #[test]
    fn invariants_are_enforced() {
        let mut p = project();
        let [a, b, c] = chain3(&mut p);
        let d = add(&mut p, 67, 3.0, 4.0);
        assert_eq!(p.link_notes(a, d, LinkType::Slide), Err(LinkError::HasOutgoing(a)));
        assert_eq!(p.link_notes(c, b, LinkType::Slide), Err(LinkError::NotInOrder));
        assert_eq!(p.link_notes(d, b, LinkType::Slide), Err(LinkError::NotInOrder));
        let e = add(&mut p, 69, 2.5, 3.5);
        assert_eq!(p.link_notes(e, c, LinkType::Slide), Err(LinkError::NotInOrder));
        assert_eq!(p.link_notes(b, e, LinkType::Slide), Err(LinkError::HasOutgoing(b)));
        let f = add(&mut p, 70, 5.0, 6.0);
        assert_eq!(p.link_notes(f, c, LinkType::Slide), Err(LinkError::NotInOrder));
        assert_eq!(p.link_notes(c, c, LinkType::Slide), Err(LinkError::SameNote));
        assert_eq!(p.link_notes(c, 99, LinkType::Slide), Err(LinkError::UnknownNote(99)));
        assert_eq!(p.link_notes(c, d, LinkType::Slide), Ok(()));
        assert_eq!(p.link_notes(e, d, LinkType::Slide), Err(LinkError::HasIncoming(d)));
        let same = add(&mut p, 64, 6.0, 7.0);
        assert_eq!(p.link_notes(f, same, LinkType::Slide), Ok(()));
        let twin = add(&mut p, 70, 7.0, 8.0);
        assert_eq!(p.link_notes(same, twin, LinkType::Slide), Ok(()));
        let flat = add(&mut p, 70, 7.5, 8.0);
        assert_eq!(p.link_notes(twin, flat, LinkType::Slide), Err(LinkError::SamePitch));
    }

    #[test]
    fn cycles_are_rejected() {
        let mut p = project();
        let a = add(&mut p, 60, 0.0, 1.0);
        let b = add(&mut p, 62, 0.0, 1.0);
        p.link_notes(a, b, LinkType::Slide).unwrap();
        assert_eq!(p.link_notes(b, a, LinkType::Slide), Err(LinkError::Cycle));
    }

    #[test]
    fn type_rules() {
        let mut p = project();
        let a = add(&mut p, 60, 0.0, 1.0);
        let far = add(&mut p, 66, 1.0, 2.0);
        assert_eq!(p.link_notes(a, far, LinkType::Bend), Err(LinkError::BendInterval));
        let low = add(&mut p, 40, 3.0, 3.5);
        let high = add(&mut p, 65, 3.5, 4.0);
        assert_eq!(p.link_notes(low, high, LinkType::Legato), Err(LinkError::NoSameString));
        assert_eq!(p.infer_link_type(low, high), LinkType::Slide);
        assert_eq!(p.infer_link_type(a, far), LinkType::Legato);
        let gap = add(&mut p, 62, 3.9, 4.0);
        assert_eq!(p.link_notes(a, gap, LinkType::Legato), Err(LinkError::NotAdjacent));
        p.link_notes(low, high, LinkType::Slide).unwrap();
        assert!(!p.chain_fits_one_string(low));
        assert!(p.chain_fits_one_string(a));
    }

    #[test]
    fn unlink_and_split_chain() {
        let mut p = project();
        let [a, b, c] = chain3(&mut p);
        assert!(p.unlink(a));
        assert!(!p.unlink(a));
        assert_eq!(p.chain_of(b), [b, c]);
        assert!(p.note(a).unwrap().technique.is_none());
        p.link_notes(a, b, LinkType::Legato).unwrap();
        assert!(p.split_chain(c));
        assert_eq!(p.chain_of(a), [a, b]);
        assert!(!p.split_chain(a));
        assert!(p.unlink(a));
        assert_eq!(p.note(b).unwrap().legato, None);
    }

    #[test]
    fn cycle_walks_slide_bend_legato_and_skips_invalid_states() {
        let mut p = project();
        let (a, b) = (add(&mut p, 60, 0.0, 1.0), add(&mut p, 62, 1.0, 2.0));
        p.link_notes(a, b, LinkType::Slide).unwrap();
        assert_eq!(p.cycle_link_type(a), Some(LinkType::Bend));
        assert_eq!(p.cycle_link_type(a), Some(LinkType::Legato));
        assert_eq!(p.cycle_link_type(a), Some(LinkType::Slide));
        assert_eq!(p.outgoing(a), Some((b, LinkType::Slide)));

        let c = add(&mut p, 70, 2.0, 3.0);
        p.link_notes(b, c, LinkType::Slide).unwrap();
        assert_eq!(p.cycle_link_type(b), Some(LinkType::Legato));
        assert_eq!(p.cycle_link_type(b), Some(LinkType::Slide));

        let d = add(&mut p, 40, 4.0, 5.0);
        let e = add(&mut p, 65, 5.0, 6.0);
        p.link_notes(d, e, LinkType::Slide).unwrap();
        assert_eq!(p.cycle_link_type(d), None);
        assert_eq!(p.outgoing(d), Some((e, LinkType::Slide)));
        assert_eq!(p.cycle_link_type(e), None);
    }

    #[test]
    fn deleting_the_ends_and_the_middle_of_a_chain() {
        let mut p = project();
        let [a, b, c] = chain3(&mut p);
        p.delete_notes(&[a], false);
        assert_eq!(p.chain_of(b), [b, c]);
        let mut p = project();
        let [a, b, c] = chain3(&mut p);
        p.delete_notes(&[c], false);
        assert_eq!(p.chain_of(a), [a, b]);
        assert_eq!(p.note(b).unwrap().link_to, None);
        assert_eq!(p.note(b).unwrap().technique, None);
        let mut p = project();
        let [a, b, c] = chain3(&mut p);
        p.delete_notes(&[b], false);
        assert_eq!((p.chain_of(a), p.chain_of(c)), (vec![a], vec![c]));
    }

    #[test]
    fn bridging_reconnects_the_neighbours_and_undoes_in_one_step() {
        let mut p = project();
        let [a, b, c] = chain3(&mut p);
        let before = p.notes.clone();
        let edit = p.delete_notes(&[b], true);
        assert_eq!(p.chain_of(a), [a, c]);
        assert_eq!(p.outgoing(a), Some((c, LinkType::Slide)));
        edit.undo(&mut p);
        assert_eq!(p.notes, before);
        edit.redo(&mut p);
        assert_eq!(p.chain_of(a), [a, c]);
    }

    #[test]
    fn bridging_over_several_deleted_notes() {
        let mut p = project();
        let ids = [add(&mut p, 60, 0.0, 1.0), add(&mut p, 62, 1.0, 2.0), add(&mut p, 64, 2.0, 3.0), add(&mut p, 67, 3.0, 4.0)];
        for w in ids.windows(2) {
            p.link_notes(w[0], w[1], LinkType::Slide).unwrap();
        }
        p.delete_notes(&[ids[1], ids[2]], true);
        assert_eq!(p.chain_of(ids[0]), [ids[0], ids[3]]);
    }

    #[test]
    fn link_selection_chains_single_notes_in_time_order() {
        let mut p = project();
        let ids = [add(&mut p, 62, 1.0, 2.0), add(&mut p, 60, 0.0, 1.0), add(&mut p, 64, 2.0, 3.0)];
        let report = p.link_selection(&ids);
        assert_eq!((report.linked, report.skipped.len()), (2, 0));
        assert_eq!(p.chain_of(ids[0]), [ids[1], ids[0], ids[2]]);
        assert_eq!(p.outgoing(ids[1]), Some((ids[0], LinkType::Legato)));
    }

    #[test]
    fn link_selection_pairs_chords_by_pitch_order() {
        let mut p = project();
        let low = [add(&mut p, 57, 0.0, 1.0), add(&mut p, 64, 0.0, 1.0)];
        let high = [add(&mut p, 66, 1.0, 2.0), add(&mut p, 59, 1.0, 2.0)];
        let all = [low[1], low[0], high[0], high[1]];
        let report = p.link_selection(&all);
        assert_eq!(report.linked, 2);
        assert_eq!(p.chain_of(low[0]), [low[0], high[1]]);
        assert_eq!(p.chain_of(low[1]), [low[1], high[0]]);
    }

    #[test]
    fn link_selection_with_unequal_groups_links_only_existing_pairs() {
        let mut p = project();
        let a = [add(&mut p, 57, 0.0, 1.0), add(&mut p, 64, 0.0, 1.0), add(&mut p, 69, 0.0, 1.0)];
        let b = [add(&mut p, 59, 1.0, 2.0), add(&mut p, 66, 1.0, 2.0)];
        let ids: Vec<u64> = a.iter().chain(&b).copied().collect();
        let report = p.link_selection(&ids);
        assert_eq!(report.linked, 2);
        assert_eq!(p.outgoing(a[2]), None);
    }

    #[test]
    fn overlapping_notes_are_never_linked_to_each_other() {
        let mut p = project();
        let ids = [add(&mut p, 60, 0.0, 2.0), add(&mut p, 62, 1.0, 3.0)];
        let report = p.link_selection(&ids);
        assert_eq!((report.linked, report.skipped.len()), (0, 0));
        assert!(p.outgoing(ids[0]).is_none());
    }

    #[test]
    fn invalid_pairs_are_skipped_with_a_reason_and_valid_ones_still_link() {
        let mut p = project();
        let ids = [add(&mut p, 60, 0.0, 1.0), add(&mut p, 60, 1.0, 2.0), add(&mut p, 64, 2.0, 3.0)];
        let report = p.link_selection(&ids);
        assert_eq!(report.linked, 1);
        assert_eq!(report.skipped, [(ids[0], ids[1], LinkError::SamePitch)]);
    }

    #[test]
    fn link_selection_is_one_undoable_edit() {
        let mut p = project();
        let ids = [add(&mut p, 60, 0.0, 1.0), add(&mut p, 62, 1.0, 2.0), add(&mut p, 64, 2.0, 3.0)];
        let before = p.notes.clone();
        let edit = p.edit_notes("Link", |p| {
            p.link_selection(&ids);
            Ok(())
        });
        let edit = edit.unwrap();
        assert_ne!(p.notes, before);
        edit.undo(&mut p);
        assert_eq!(p.notes, before);
    }
}
