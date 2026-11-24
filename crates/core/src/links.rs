//! Links between notes: slide and bend live on the start note (`link_to` + `technique`), legato on
//! the end note (`legato.from`). A chain is a simple linked list in time order.
//!
//! A linked note A always ends where its destination B starts (`write_link` ties it, `tie_links`
//! keeps it so after edits); on-the-beat grace notes keep their own length. A grace note that has no
//! other link is joined to the note it leads into by the implicit `Dashed` link.

use std::collections::{HashMap, HashSet};

use thiserror::Error;

use crate::tempo::EPS;
use crate::edit::ProjectEdit;
use crate::model::{GraceKind, Legato, LegatoLink, Note, Project, Technique, Tuning};
use crate::overlap::MIN_PIECE_BEATS;
use crate::tempo::{NoteTimes, TempoMap};

pub const MAX_FRET: u8 = 24;
const BEND_MAX_SEMITONES: u8 = 4;
const LEGATO_MAX_GAP_BEATS: f64 = 0.25;
const MIN_TIED_SEC: f64 = 1e-3;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LinkType {
    Slide,
    Bend,
    Legato,
    /// Same pitch, adjacent notes: one sustained sound, so a slide can still follow it.
    Tied,
    /// Grace notes only: no technique, a dashed connector. Stored as no link data.
    Dashed,
}

impl LinkType {
    pub const CYCLE: [Self; 5] = [Self::Slide, Self::Bend, Self::Legato, Self::Tied, Self::Dashed];

    fn of_technique(technique: Option<Technique>) -> Self {
        match technique {
            Some(Technique::Bend) => Self::Bend,
            Some(Technique::Tie) => Self::Tied,
            _ => Self::Slide,
        }
    }
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
    #[error("a tie needs two notes of the same pitch")]
    TiedNeedsSamePitch,
    #[error("a bend spans 1 to 4 semitones")]
    BendInterval,
    #[error("no placement on one string")]
    NoSameString,
    #[error("the notes are not adjacent")]
    NotAdjacent,
    #[error("there is no earlier note to link from")]
    NoPrevious,
    #[error("note {0} is dead or a harmonic and takes no link")]
    Ineligible(u64),
    #[error("only grace notes can have a dashed link")]
    DashedNeedsGrace,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct LinkReport {
    pub linked: usize,
    pub skipped: Vec<(u64, u64, LinkError)>,
}

/// Legato when a same-string placement exists, slide otherwise.
pub fn infer_link(tuning: &Tuning, capo: u8, from_midi: u8, to_midi: u8) -> LinkType {
    if from_midi == to_midi {
        LinkType::Tied
    } else if same_string_fit(tuning, capo, &[from_midi, to_midi]) {
        LinkType::Legato
    } else {
        LinkType::Slide
    }
}

/// Whether one string holds every pitch between the capo and `MAX_FRET`.
pub fn same_string_fit(tuning: &Tuning, capo: u8, midis: &[u8]) -> bool {
    tuning.open_strings.iter().any(|&open| {
        midis.iter().all(|&m| {
            m.checked_sub(open)
                .is_some_and(|fret| (capo..=MAX_FRET).contains(&fret))
        })
    })
}

/// Whether the end of `a` is tied to the start of its destination (not for on-the-beat grace notes).
pub(crate) fn ties_end(a: &Note) -> bool {
    !matches!(a.grace_of, Some(g) if g.kind == GraceKind::Appoggiatura)
}

fn adjacent(tempo: &TempoMap, a: &Note, b: &Note) -> bool {
    let gap = tempo
        .pos_to_beats(b.start)
        .zip(tempo.pos_to_beats(a.end))
        .map(|(s, e)| (s - e).abs());
    gap.is_none_or(|g| g <= LEGATO_MAX_GAP_BEATS)
}

/// Both notes carry a string assignment and it differs.
pub(crate) fn strings_differ(a: &Note, b: &Note) -> bool {
    matches!((a.string_fret, b.string_fret), (Some((x, _)), Some((y, _))) if x != y)
}

pub(crate) fn legato_kind(from: &Note, to: &Note) -> Legato {
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
            return Some((to, LinkType::of_technique(note.technique)));
        }
        self.notes
            .iter()
            .find(|n| n.legato.is_some_and(|l| l.from == id))
            .map(|n| (n.id, LinkType::Legato))
    }

    /// The link that starts at `id`: a real one, or for a grace note the implicit dashed connector
    /// to the note it leads into.
    pub fn link_of(&self, id: u64) -> Option<(u64, LinkType)> {
        self.outgoing(id).or_else(|| {
            self.note(id)?
                .grace_of
                .and(self.grace_target(id))
                .map(|to| (to, LinkType::Dashed))
        })
    }

    pub fn incoming(&self, id: u64) -> Option<(u64, LinkType)> {
        if let Some(l) = self.note(id)?.legato {
            return Some((l.from, LinkType::Legato));
        }
        self.notes
            .iter()
            .find(|n| n.link_to == Some(id))
            .map(|n| (n.id, LinkType::of_technique(n.technique)))
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
        while let Some((next, _)) = self
            .outgoing(*chain.last().unwrap_or(&head))
            .filter(|(n, _)| seen.insert(*n))
        {
            chain.push(next);
        }
        chain
    }

    pub fn chain_fits_one_string(&self, id: u64) -> bool {
        let midis: Vec<u8> = self
            .chain_of(id)
            .iter()
            .filter_map(|&n| self.note(n))
            .map(|n| n.midi)
            .collect();
        same_string_fit(&self.tuning, self.capo, &midis)
    }

    /// Legato when the notes are adjacent, not assigned to different strings and a same-string
    /// placement exists, slide otherwise.
    pub fn infer_link_type(&self, from: u64, to: u64) -> LinkType {
        let tempo = TempoMap::new(&self.measures);
        match (self.note(from), self.note(to)) {
            (Some(a), Some(b)) if adjacent(&tempo, a, b) && !strings_differ(a, b) => {
                infer_link(&self.tuning, self.capo, a.midi, b.midi)
            }
            _ => LinkType::Slide,
        }
    }

    pub fn can_link(&self, from: u64, to: u64, kind: LinkType) -> Result<(), LinkError> {
        let tempo = TempoMap::new(&self.measures);
        self.check_link(
            from,
            to,
            kind,
            &NoteTimes::build(&self.notes, &tempo),
            &tempo,
        )
    }

    fn check_link(
        &self,
        from: u64,
        to: u64,
        kind: LinkType,
        times: &NoteTimes,
        tempo: &TempoMap,
    ) -> Result<(), LinkError> {
        let (a, b) = (
            self.note(from).ok_or(LinkError::UnknownNote(from))?,
            self.note(to).ok_or(LinkError::UnknownNote(to))?,
        );
        if from == to {
            return Err(LinkError::SameNote);
        }
        if kind == LinkType::Dashed {
            return if a.grace_of.is_some() {
                Ok(())
            } else {
                Err(LinkError::DashedNeedsGrace)
            };
        }
        if let Some(bad) = [a, b].into_iter().find(|n| n.technique.is_some_and(Technique::blocks_links)) {
            return Err(LinkError::Ineligible(bad.id));
        }
        let (sa, sb) = (
            times.start_sec(from).unwrap_or(0.0),
            times.start_sec(to).unwrap_or(0.0),
        );
        if sb < sa - EPS || (ties_end(a) && sb < sa + MIN_TIED_SEC) {
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
        if kind == LinkType::Tied {
            if interval != 0 || a.grace_of.is_some() {
                return Err(LinkError::TiedNeedsSamePitch);
            }
            return if ties_end(a) && !adjacent(tempo, a, b) { Err(LinkError::NotAdjacent) } else { Ok(()) };
        }
        if interval == 0 {
            return Err(LinkError::SamePitch);
        }
        match kind {
            LinkType::Slide | LinkType::Dashed | LinkType::Tied => {}
            LinkType::Bend if !(1..=BEND_MAX_SEMITONES).contains(&interval) => {
                return Err(LinkError::BendInterval)
            }
            LinkType::Bend => {}
            LinkType::Legato => {
                if !same_string_fit(&self.tuning, self.capo, &[a.midi, b.midi]) {
                    return Err(LinkError::NoSameString);
                }
                if ties_end(a) && !adjacent(tempo, a, b) {
                    return Err(LinkError::NotAdjacent);
                }
            }
        }
        Ok(())
    }

    /// Writes the link and ties the end of `from` to the start of `to`.
    pub(crate) fn write_link(&mut self, from: u64, to: u64, kind: LinkType) {
        let Some((legato, start)) = self
            .note(from)
            .zip(self.note(to))
            .map(|(a, b)| (legato_kind(a, b), b.start))
        else {
            return;
        };
        if kind == LinkType::Dashed {
            return;
        }
        for n in &mut self.notes {
            if n.id == from {
                if kind != LinkType::Legato {
                    n.link_to = Some(to);
                    n.technique = Some(match kind {
                        LinkType::Bend => Technique::Bend,
                        LinkType::Tied => Technique::Tie,
                        _ => Technique::Slide,
                    });
                }
                if ties_end(n) {
                    n.end = start;
                }
            } else if n.id == to && kind == LinkType::Legato {
                n.legato = Some(LegatoLink { kind: legato, from });
            }
        }
    }

    /// Re-ties the links of the `touched` notes after an edit: the end that was not touched follows
    /// (both touched: the source's end follows the destination's start). A link that cannot hold
    /// any more (a note would shrink below one lattice unit) is dropped. Returns the notes whose
    /// edge moved.
    pub fn tie_links(&mut self, touched: &[u64]) -> Vec<u64> {
        let tempo = TempoMap::new(&self.measures);
        let at = |p| tempo.pos_to_beats(p).unwrap_or(0.0);
        let touched: HashSet<u64> = touched.iter().copied().collect();
        let index: HashMap<u64, usize> = self.notes.iter().enumerate().map(|(i, n)| (n.id, i)).collect();
        let mut legato_target: HashMap<u64, u64> = HashMap::new();
        for n in &self.notes {
            if let Some(l) = n.legato {
                legato_target.entry(l.from).or_insert(n.id);
            }
        }
        let links: Vec<(u64, u64)> = self
            .notes
            .iter()
            .filter_map(|n| n.link_to.or_else(|| legato_target.get(&n.id).copied()).map(|to| (n.id, to)))
            .filter(|(from, to)| touched.contains(from) || touched.contains(to))
            .collect();
        let mut moved = Vec::new();
        for (from, to) in links {
            let (Some(&fi), Some(&ti)) = (index.get(&from), index.get(&to)) else {
                continue;
            };
            let (a, b) = (self.notes[fi].clone(), self.notes[ti].clone());
            if !ties_end(&a) || (at(a.end) - at(b.start)).abs() < EPS {
                continue;
            }
            let end_follows = touched.contains(&to);
            let room = if end_follows {
                at(b.start) - at(a.start)
            } else {
                at(b.end) - at(a.end)
            };
            if room < MIN_PIECE_BEATS {
                self.unlink(from);
                continue;
            }
            let (id, i) = if end_follows { (from, fi) } else { (to, ti) };
            if end_follows {
                self.notes[i].end = b.start;
            } else {
                self.notes[i].start = a.end;
            }
            moved.push(id);
        }
        moved
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

    /// The kinds the link that starts at `id` could take now (its current one included), in
    /// cycling order. Empty when there is no link.
    pub fn valid_links(&self, id: u64) -> Vec<LinkType> {
        let Some((to, _)) = self.link_of(id) else {
            return Vec::new();
        };
        let mut scratch = self.clone();
        scratch.unlink(id);
        let tempo = TempoMap::new(&scratch.measures);
        let times = NoteTimes::build(&scratch.notes, &tempo);
        LinkType::CYCLE
            .into_iter()
            .filter(|&k| scratch.check_link(id, to, k, &times, &tempo).is_ok())
            .collect()
    }

    /// Replaces the link that starts at `id` by another kind (not checked).
    pub(crate) fn set_link(&mut self, id: u64, kind: LinkType) {
        if let Some((to, _)) = self.link_of(id) {
            self.unlink(id);
            self.write_link(id, to, kind);
        }
    }

    /// Sets the kind of the link that starts at `id`; `false` when there is no link or `kind` is not valid for it.
    pub fn set_link_type(&mut self, id: u64, kind: LinkType) -> bool {
        let ok = self.valid_links(id).contains(&kind);
        if ok {
            self.set_link(id, kind);
        }
        ok
    }

    /// Next valid kind of the link of `id` in the order slide, bend, legato, dashed (grace notes
    /// only); `None` when there is no link or no other kind is valid.
    pub fn cycle_link_type(&mut self, id: u64) -> Option<LinkType> {
        let (_, current) = self.link_of(id)?;
        let valid = self.valid_links(id);
        let at = LinkType::CYCLE.iter().position(|&k| k == current)?;
        let next = (1..LinkType::CYCLE.len())
            .map(|step| LinkType::CYCLE[(at + step) % LinkType::CYCLE.len()])
            .find(|k| valid.contains(k))?;
        self.set_link(id, next);
        Some(next)
    }

    /// Links consecutive groups of overlapping notes pair by pair in pitch order, inferring the type.
    pub fn link_selection(&mut self, ids: &[u64]) -> LinkReport {
        self.link_pairs(ids, false)
    }

    /// Like `link_selection` but every pair becomes a legato (`force_legato`, no slide fallback);
    /// a single note is linked from the note before it.
    pub fn force_legato_selection(&mut self, ids: &[u64]) -> LinkReport {
        self.link_pairs(ids, true)
    }

    /// The note starting latest before `id` (ties: the closest pitch).
    pub fn previous_note(&self, id: u64) -> Option<u64> {
        let tempo = TempoMap::new(&self.measures);
        let times = NoteTimes::build(&self.notes, &tempo);
        let (me, start) = (self.note(id)?, times.start_sec(id)?);
        self.notes
            .iter()
            .filter(|n| times.start_sec(n.id).is_some_and(|s| s < start - EPS))
            .max_by(|a, b| {
                let key = |n: &Note| {
                    (
                        times.start_sec(n.id).unwrap_or(f64::MIN),
                        -i32::from(n.midi.abs_diff(me.midi)),
                    )
                };
                key(a)
                    .partial_cmp(&key(b))
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
            .map(|n| n.id)
    }

    fn link_pairs(&mut self, ids: &[u64], forced: bool) -> LinkReport {
        if let ([id], true) = (ids, forced) {
            let mut report = LinkReport::default();
            match self.previous_note(*id) {
                Some(prev) => match self.force_legato(prev, *id) {
                    Ok(_) => report.linked = 1,
                    Err(e) => report.skipped.push((prev, *id, e.into())),
                },
                None => report.skipped.push((*id, *id, LinkError::NoPrevious)),
            }
            return report;
        }
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
            let (start, end) = (
                times.start_sec(n.id).unwrap_or(0.0),
                times.end_sec(n.id).unwrap_or(0.0),
            );
            if groups.is_empty() || start >= group_end - EPS {
                groups.push(Vec::new());
                group_end = end;
            }
            group_end = group_end.max(end);
            if let Some(group) = groups.last_mut() {
                group.push(n.id);
            }
        }
        let by_pitch = |group: &mut Vec<u64>, p: &Project| {
            group.sort_by_key(|&id| p.note(id).map_or(0, |n| n.midi))
        };
        for group in &mut groups {
            by_pitch(group, self);
        }
        let mut report = LinkReport::default();
        for pair in groups.windows(2) {
            for (&from, &to) in pair[0].iter().zip(&pair[1]) {
                let result = if forced {
                    self.force_legato(from, to)
                        .map(drop)
                        .map_err(LinkError::from)
                } else {
                    let kind = self.infer_link_type(from, to);
                    self.check_link(from, to, kind, &times, &tempo)
                        .map(|()| self.write_link(from, to, kind))
                };
                match result {
                    Ok(()) => report.linked += 1,
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
                        let (first, kind) =
                            p.outgoing(n.id).filter(|(to, _)| doomed.contains(to))?;
                        let mut next = first;
                        while let Some((after, _)) =
                            p.outgoing(next).filter(|_| doomed.contains(&next))
                        {
                            next = after;
                        }
                        (!doomed.contains(&next)).then_some((n.id, next, kind))
                    })
                    .collect()
            } else {
                Vec::new()
            };
            p.remove_note_set(&doomed);
            let mut sources = Vec::new();
            for (from, to, kind) in bridges {
                let linked = p.link_notes(from, to, kind).is_ok() || {
                    let inferred = p.infer_link_type(from, to);
                    p.link_notes(from, to, inferred).is_ok()
                };
                if linked {
                    sources.push(from);
                }
            }
            p.resolve_overlaps(&sources);
        })
    }
}

#[cfg(all(test, dev_tests))]
#[path = "../../../dev/tests/unit/core/links.rs"]
mod tests;
