//! No two notes of the same pitch overlap: the note that is placed wins over the ones it lands on.

use std::collections::HashSet;

use crate::tempo::EPS;
use crate::model::{Note, Technique};
use crate::tempo::TempoMap;
use crate::tracks::next_note_id;
use crate::Project;

/// Shortest piece of a note that survives being run over (one lattice unit).
pub const MIN_PIECE_BEATS: f64 = 1.0 / 24.0;

impl Project {
    /// First re-ties the links of the `placed` notes (`tie_links`; the notes they link to count as
    /// placed too). Then makes room for them: any other note of the same pitch they overlap is cut
    /// back (the cut edge moves to the placed note, and the link on that side is dropped), split in
    /// two when the placed note lies in its middle (the tail takes over the outgoing links), or
    /// removed when no piece of at least [`MIN_PIECE_BEATS`] is left; an overlapped grace note is
    /// removed. Notes of other pitches and the placed notes themselves are never touched. Call it
    /// inside the edit that placed the notes.
    pub fn resolve_overlaps(&mut self, placed: &[u64]) {
        let mut placed = placed.to_vec();
        let moved = self.tie_links(&placed);
        for id in placed.clone() {
            let partners = [self.outgoing(id), self.incoming(id)]
                .into_iter()
                .flatten()
                .map(|(p, _)| p);
            placed.extend(partners);
        }
        placed.extend(moved);
        placed.sort_unstable();
        placed.dedup();
        let tempo = TempoMap::new(&self.measures);
        let beats = |p| tempo.pos_to_beats(p).unwrap_or(0.0);
        let keep: HashSet<u64> = placed.iter().copied().collect();
        let mut doomed = HashSet::new();
        for &pid in &placed {
            let Some(p) = self.note(pid).cloned() else {
                continue;
            };
            let (ps, pe) = (beats(p.start), beats(p.end));
            let hits: Vec<Note> = self
                .notes
                .iter()
                .filter(|o| o.midi == p.midi && !keep.contains(&o.id) && !doomed.contains(&o.id))
                .filter(|o| beats(o.start) < pe - EPS && beats(o.end) > ps + EPS)
                .cloned()
                .collect();
            for o in hits {
                let head = ps - beats(o.start) >= MIN_PIECE_BEATS - EPS;
                let tail = beats(o.end) - pe >= MIN_PIECE_BEATS - EPS;
                match (head, tail) {
                    (false, false) => {
                        doomed.insert(o.id);
                    }
                    _ if o.grace_of.is_some() => {
                        doomed.insert(o.id);
                    }
                    (true, false) => {
                        self.unlink(o.id);
                        self.edit_note(o.id, |n| n.end = p.start);
                    }
                    (false, true) => {
                        self.split_chain(o.id);
                        self.edit_note(o.id, |n| n.start = p.end);
                    }
                    (true, true) => self.split(&o, &p),
                }
            }
        }
        if !doomed.is_empty() {
            let all = self.with_grace_notes(&doomed.iter().copied().collect::<Vec<_>>());
            self.remove_note_set(&all);
        }
    }

    fn edit_note(&mut self, id: u64, f: impl FnOnce(&mut Note)) {
        if let Some(n) = self.notes.iter_mut().find(|n| n.id == id) {
            f(n);
        }
    }

    /// `o` keeps the part before `p`; the part after it becomes a new note that takes over the
    /// links that left `o`.
    fn split(&mut self, o: &Note, p: &Note) {
        let id = next_note_id(self);
        let mut tail = o.clone();
        (tail.id, tail.start, tail.legato) = (id, p.end, None);
        self.edit_note(o.id, |n| {
            n.end = p.start;
            if n.link_to.take().is_some() && n.technique.is_some_and(Technique::is_link)
            {
                n.technique = None;
            }
        });
        for n in &mut self.notes {
            if let Some(l) = n.legato.as_mut().filter(|l| l.from == o.id) {
                l.from = id;
            }
        }
        self.notes.push(tail);
    }
}

#[cfg(all(test, dev_tests))]
#[path = "../../../dev/tests/unit/core/overlap.rs"]
mod tests;
