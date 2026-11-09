//! App-side clipboard: notes with positions relative to the earliest one.

use std::collections::HashMap;

use fretscribe_core::{EditError, LegatoLink, Note, Project, ProjectEdit, Technique, TempoMap, TimePos};

use super::modify::with_graces;
use crate::spectro::{GhostGlyph, GhostShape, LinkKind};

#[derive(Clone, Debug, PartialEq)]
struct ClipNote {
    note: Note,
    start_off: f64,
    end_off: f64,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Clipboard {
    notes: Vec<ClipNote>,
    origin_beats: f64,
}

impl Clipboard {
    /// Links between copied notes are kept, links to other notes dropped; grace notes travel with
    /// their main note.
    pub fn copy(project: &Project, tempo: &TempoMap, ids: &[u64]) -> Option<Self> {
        let all = with_graces(&project.notes, ids);
        let chosen: Vec<&Note> = project.notes.iter().filter(|n| all.contains(&n.id) && n.grace_of.is_none_or(|g| all.contains(&g.main))).collect();
        let beats = |p: TimePos| tempo.pos_to_beats(p);
        let origin = chosen.iter().filter_map(|n| beats(n.start)).min_by(f64::total_cmp)?;
        let notes = chosen
            .iter()
            .map(|n| Some(ClipNote { note: (*n).clone(), start_off: beats(n.start)? - origin, end_off: beats(n.end)? - origin }))
            .collect::<Option<Vec<_>>>()?;
        Some(Self { notes, origin_beats: origin })
    }

    pub fn is_empty(&self) -> bool {
        self.notes.is_empty()
    }

    /// Where a duplicate lands: right after the latest copied end.
    pub fn end_pos(&self, tempo: &TempoMap) -> TimePos {
        let end = self.notes.iter().map(|c| c.end_off).fold(0.0, f64::max);
        tempo.beats_to_pos(self.origin_beats + end)
    }

    /// Notes with fresh ids from `first_id`, the earliest one at `anchor`.
    pub fn paste_notes(&self, tempo: &TempoMap, anchor: TimePos, first_id: u64) -> Vec<Note> {
        let base = tempo.pos_to_beats(anchor).unwrap_or(self.origin_beats);
        let fresh: HashMap<u64, u64> = self.notes.iter().zip(first_id..).map(|(c, id)| (c.note.id, id)).collect();
        self.notes
            .iter()
            .map(|c| {
                let mut n = c.note.clone();
                n.id = fresh[&c.note.id];
                n.start = tempo.beats_to_pos(base + c.start_off);
                n.end = tempo.beats_to_pos(base + c.end_off);
                n.link_to = n.link_to.and_then(|to| fresh.get(&to).copied());
                if n.link_to.is_none() && matches!(n.technique, Some(Technique::Slide | Technique::Bend)) {
                    n.technique = None;
                }
                n.legato = n.legato.and_then(|l| fresh.get(&l.from).map(|&from| LegatoLink { from, ..l }));
                n.grace_of = n.grace_of.and_then(|g| fresh.get(&g.main).map(|&main| fretscribe_core::GraceNote { main, ..g }));
                n
            })
            .collect()
    }

    pub fn ghost(&self, tempo: &TempoMap, anchor: TimePos) -> GhostShape {
        let notes = self.paste_notes(tempo, anchor, 0);
        let index: HashMap<u64, usize> = notes.iter().enumerate().map(|(i, n)| (n.id, i)).collect();
        let glyphs = notes
            .iter()
            .map(|n| GhostGlyph {
                midi: n.midi,
                start_sec: tempo.pos_to_sec(n.start),
                end_sec: tempo.pos_to_sec(n.end),
                link_to: n.link_to.and_then(|to| index.get(&to)).map(|&i| (i, if n.technique == Some(Technique::Bend) { LinkKind::Bend } else { LinkKind::Slide })),
                legato_from: n.legato.and_then(|l| index.get(&l.from).copied()),
            })
            .collect();
        GhostShape { glyphs }
    }

    /// Pastes as one undoable edit; returns the new ids.
    pub fn commit(&self, project: &mut Project, tempo: &TempoMap, anchor: TimePos, label: &str) -> Result<(ProjectEdit, Vec<u64>), EditError> {
        let first = fretscribe_core::next_note_id(project);
        let notes = self.paste_notes(tempo, anchor, first);
        let ids: Vec<u64> = notes.iter().map(|n| n.id).collect();
        let edit = project.edit_notes(label, |p| {
            p.notes.extend(notes);
            p.resolve_overlaps(&ids);
            Ok(())
        })?;
        Ok((edit, ids))
    }
}

#[cfg(test)]
mod tests {
    use super::super::testkit::*;
    use super::*;
    use fretscribe_core::{GraceKind, GraceNote, LinkType};

    #[test]
    fn copy_keeps_relative_timing_and_internal_links_and_paste_gives_fresh_ids() {
        let mut p = project();
        let (a, b, outside) = (add(&mut p, 60, 1.0, 2.0), add(&mut p, 62, 2.5, 3.0), add(&mut p, 64, 3.0, 4.0));
        p.link_notes(a, b, LinkType::Slide).unwrap();
        p.link_notes(b, outside, LinkType::Slide).unwrap();
        let tempo = TempoMap::new(&p.measures);
        let clip = Clipboard::copy(&p, &tempo, &[a, b]).unwrap();
        let (edit, ids) = clip.commit(&mut p, &tempo, at(5.0), "Paste").unwrap();
        assert_eq!(ids.len(), 2);
        assert!(ids.iter().all(|id| ![a, b, outside].contains(id)));
        let (na, nb) = (p.note(ids[0]).unwrap(), p.note(ids[1]).unwrap());
        let beats = |p| tempo.pos_to_beats(p).unwrap();
        assert_eq!((beats(na.start), beats(na.end)), (5.0, 6.0));
        assert_eq!((beats(nb.start), beats(nb.end)), (6.5, 7.0));
        assert_eq!(p.outgoing(ids[0]), Some((ids[1], LinkType::Slide)));
        assert_eq!(p.outgoing(ids[1]), None);
        assert_eq!(p.note(ids[1]).unwrap().technique, None);
        assert_eq!(p.outgoing(b), Some((outside, LinkType::Slide)));
        edit.undo(&mut p);
        assert_eq!(p.notes.len(), 3);
    }

    #[test]
    fn grace_notes_travel_with_their_main_note_and_orphans_stay_behind() {
        let mut p = project();
        let (main, grace, lone) = (add(&mut p, 60, 1.0, 2.0), add(&mut p, 62, 0.9, 1.0), add(&mut p, 64, 3.0, 4.0));
        for id in [grace, lone] {
            let m = if id == grace { main } else { 999 };
            p.notes.iter_mut().find(|n| n.id == id).unwrap().grace_of = Some(GraceNote { main: m, kind: GraceKind::Acciaccatura });
        }
        let tempo = TempoMap::new(&p.measures);
        let clip = Clipboard::copy(&p, &tempo, &[main, lone]).unwrap();
        let notes = clip.paste_notes(&tempo, at(8.0), 100);
        assert_eq!(notes.len(), 2);
        let g = notes.iter().find(|n| n.grace_of.is_some()).unwrap();
        assert_eq!(g.grace_of.unwrap().main, 100);
        assert!((tempo.pos_to_beats(g.start).unwrap() - 8.0).abs() < 1e-9);
    }

    #[test]
    fn ghost_shows_the_pasted_links_and_duplicate_lands_after_the_selection() {
        let mut p = project();
        let (a, b) = (add(&mut p, 60, 1.0, 2.0), add(&mut p, 62, 2.0, 3.0));
        p.link_notes(a, b, LinkType::Legato).unwrap();
        let tempo = TempoMap::new(&p.measures);
        let clip = Clipboard::copy(&p, &tempo, &[a, b]).unwrap();
        let ghost = clip.ghost(&tempo, at(4.0));
        assert_eq!(ghost.glyphs.len(), 2);
        assert_eq!(ghost.glyphs[1].legato_from, Some(0));
        assert_eq!(ghost.glyphs[0].start_sec, sec(4.0));
        assert_eq!(clip.end_pos(&tempo), at(3.0));
    }

    #[test]
    fn copying_nothing_gives_no_clipboard() {
        let p = project();
        assert!(Clipboard::copy(&p, &TempoMap::new(&p.measures), &[]).is_none());
    }
}
