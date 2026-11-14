//! No two notes of the same pitch overlap: the note that is placed wins over the ones it lands on.

use std::collections::HashSet;

use crate::model::{Note, Technique};
use crate::tempo::TempoMap;
use crate::tracks::next_note_id;
use crate::Project;

/// Shortest piece of a note that survives being run over (one lattice unit).
pub const MIN_PIECE_BEATS: f64 = 1.0 / 24.0;
const EPS: f64 = 1e-9;

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
            if n.link_to.take().is_some()
                && matches!(n.technique, Some(Technique::Slide | Technique::Bend))
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{TimePos, TimeSignature, Track, TrackRole};
    use crate::LinkType;

    fn project() -> Project {
        Project::new(
            Track::new(1, "Guitar", TrackRole::Guitar),
            8.0,
            120.0,
            TimeSignature::COMMON,
        )
    }

    fn add(p: &mut Project, midi: u8, start: f64, end: f64) -> u64 {
        let id = next_note_id(p);
        p.notes.push(Note::new(
            id,
            midi,
            TimePos::new(1, start),
            TimePos::new(1, end),
        ));
        id
    }

    fn spans(p: &Project, midi: u8) -> Vec<(f64, f64)> {
        let mut v: Vec<_> = p
            .notes
            .iter()
            .filter(|n| n.midi == midi)
            .map(|n| (n.start.beat, n.end.beat))
            .collect();
        v.sort_by(|a, b| a.partial_cmp(b).unwrap());
        v
    }

    #[test]
    fn overlapped_notes_are_cut_split_or_removed_and_other_pitches_stay() {
        let mut p = project();
        let (a, b, c) = (
            add(&mut p, 60, 0.0, 1.0),
            add(&mut p, 60, 1.0, 2.0),
            add(&mut p, 60, 3.0, 3.5),
        );
        let other = add(&mut p, 64, 0.0, 4.0);
        let by_end = add(&mut p, 60, 0.5, 1.5);
        p.resolve_overlaps(&[by_end]);
        assert_eq!(
            spans(&p, 60),
            [(0.0, 0.5), (0.5, 1.5), (1.5, 2.0), (3.0, 3.5)]
        );
        assert!(p.note(a).is_some() && p.note(b).is_some() && p.note(c).is_some());
        assert_eq!(p.note(other).unwrap().end.beat, 4.0);

        let big = add(&mut p, 60, 0.25, 3.25);
        p.resolve_overlaps(&[big]);
        assert_eq!(spans(&p, 60), [(0.0, 0.25), (0.25, 3.25), (3.25, 3.5)]);
        assert!(p.note(b).is_none());
    }

    #[test]
    fn a_note_in_the_middle_splits_the_one_under_it_and_the_tail_keeps_the_links() {
        let mut p = project();
        let (a, long, next) = (
            add(&mut p, 60, 0.0, 1.0),
            add(&mut p, 62, 1.0, 4.0),
            add(&mut p, 65, 4.0, 5.0),
        );
        p.link_notes(a, long, LinkType::Legato).unwrap();
        p.link_notes(long, next, LinkType::Legato).unwrap();
        let mid = add(&mut p, 62, 2.0, 3.0);
        p.resolve_overlaps(&[mid]);
        assert_eq!(spans(&p, 62), [(1.0, 2.0), (2.0, 3.0), (3.0, 4.0)]);
        let tail = p
            .notes
            .iter()
            .find(|n| n.midi == 62 && n.start.beat == 3.0)
            .unwrap()
            .id;
        assert_eq!(p.note(next).unwrap().legato.map(|l| l.from), Some(tail));
        assert_eq!(p.note(long).unwrap().legato.map(|l| l.from), Some(a));
        assert!(p.note(tail).unwrap().legato.is_none());
    }

    #[test]
    fn a_split_with_a_sliver_on_one_side_drops_it_and_a_covered_note_goes_with_its_links() {
        let mut p = project();
        let (a, b) = (add(&mut p, 60, 0.0, 2.0), add(&mut p, 64, 2.0, 3.0));
        p.link_notes(a, b, LinkType::Slide).unwrap();
        let mid = add(&mut p, 60, 0.02, 1.0);
        p.resolve_overlaps(&[mid]);
        assert_eq!(spans(&p, 60), [(0.02, 1.0), (1.0, 2.0)]);
        let cover = add(&mut p, 60, 0.0, 3.0);
        p.resolve_overlaps(&[cover]);
        assert_eq!(spans(&p, 60), [(0.0, 3.0)]);
        assert_eq!(p.outgoing(a), None);
        assert!(p.note(b).is_some());
    }
}
