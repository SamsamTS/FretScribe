//! Quick entry of a grace note: modal keys over a ghost note, one undo step on Enter.

use fretscribe_core::{GraceKind, Project, ProjectEdit, MAX_FRET};
use fretscribe_notation::tab::{candidate_label, lowest_fret_candidate};

use super::draw::DrawFeedback;
use crate::spectro::{GhostGlyph, GhostShape};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EntryKey {
    /// Semitone, or a tone with `tone`.
    Pitch { up: bool, tone: bool },
    /// Neighbouring string (up = towards the higher strings).
    String { up: bool },
    Digit(u8),
    ToggleKind,
    ToggleHammer,
    Confirm,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EntryStep {
    Continue,
    Done,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GraceEntry {
    pub main: u64,
    pub midi: u8,
    pub string: Option<u8>,
    pub kind: GraceKind,
    pub hammer: bool,
    typed: Option<u8>,
}

impl GraceEntry {
    pub fn new(project: &Project, main: u64) -> Option<Self> {
        let midi = fretscribe_core::default_grace_midi(project.note(main)?.midi);
        Some(Self { main, midi, string: None, kind: GraceKind::Acciaccatura, hammer: false, typed: None })
    }

    fn fret_on(project: &Project, string: u8, midi: u8) -> Option<u8> {
        let open = *project.tuning.open_strings.get(usize::from(string))?;
        midi.checked_sub(open).filter(|f| (project.capo..=MAX_FRET).contains(f))
    }

    /// The string and fret shown: the chosen string when it holds the pitch, else the lowest fret.
    pub fn placement(&self, project: &Project) -> Option<(u8, u8)> {
        self.string
            .and_then(|s| Self::fret_on(project, s, self.midi).map(|f| (s, f)))
            .or_else(|| lowest_fret_candidate(&project.tuning, project.capo, self.midi))
    }

    pub fn key(&mut self, project: &Project, key: EntryKey) -> EntryStep {
        let typed = self.typed.take();
        match key {
            EntryKey::Confirm => return EntryStep::Done,
            EntryKey::ToggleKind => {
                self.kind = if self.kind == GraceKind::Acciaccatura { GraceKind::Appoggiatura } else { GraceKind::Acciaccatura };
            }
            EntryKey::ToggleHammer => self.hammer = !self.hammer,
            EntryKey::Pitch { up, tone } => {
                let by = if tone { 2 } else { 1 };
                self.midi = if up { self.midi.saturating_add(by).min(127) } else { self.midi.saturating_sub(by) };
            }
            EntryKey::String { up } => {
                let from = self.placement(project).map_or(0, |p| i32::from(p.0));
                let step = if up { 1 } else { -1 };
                let next = (1..=project.tuning.open_strings.len() as i32).map(|d| from + d * step).take_while(|s| (0..project.tuning.open_strings.len() as i32).contains(s)).find(|&s| Self::fret_on(project, s as u8, self.midi).is_some());
                self.string = next.map(|s| s as u8).or(self.string);
            }
            EntryKey::Digit(d) => {
                let Some((string, _)) = self.placement(project) else { return EntryStep::Continue };
                let fret = typed.map(|t| t * 10 + d).filter(|f| *f <= MAX_FRET).unwrap_or(d);
                if let (Some(open), true) = (project.tuning.open_strings.get(usize::from(string)), fret >= project.capo) {
                    self.midi = open + fret;
                    self.string = Some(string);
                    self.typed = Some(fret);
                }
            }
        }
        EntryStep::Continue
    }

    /// The ghost (grace note then main note, hammered when asked) and the "B 5" label.
    pub fn preview(&self, project: &Project) -> Option<(GhostShape, DrawFeedback)> {
        let mut scratch = project.clone();
        let id = scratch.add_grace_note(self.main, Some(self.midi), self.kind).ok()?;
        let tempo = fretscribe_core::TempoMap::new(&scratch.measures);
        let glyph = |id: u64, legato_from| scratch.note(id).map(|n| GhostGlyph { midi: n.midi, start_sec: tempo.pos_to_sec(n.start), end_sec: tempo.pos_to_sec(n.end), link_to: None, legato_from, grace: n.grace_of.is_some() });
        let glyphs = vec![glyph(id, None)?, glyph(self.main, self.hammer.then_some(0))?];
        let label = self.placement(project).and_then(|(s, f)| candidate_label(&project.tuning, s, f)).unwrap_or_else(|| "?".into());
        let feedback = DrawFeedback { labels: vec![(glyphs[0].start_sec, self.midi, label)], ..DrawFeedback::default() };
        Some((GhostShape { glyphs }, feedback))
    }

    /// Adds the grace note (string locked when one was chosen) and the optional hammer-on.
    pub fn commit(&self, project: &mut Project) -> Result<(ProjectEdit, Vec<u64>), String> {
        let placement = self.placement(project).filter(|_| self.string.is_some());
        let (edit, id) = project
            .try_edit("Add grace note", |p| {
                let id = p.add_grace_note(self.main, Some(self.midi), self.kind).map_err(|e| e.to_string())?;
                if let Some(note) = p.notes.iter_mut().find(|n| n.id == id).filter(|_| placement.is_some()) {
                    note.string_fret = placement;
                    note.locked_fret = true;
                }
                if self.hammer {
                    p.force_legato(id, self.main).map_err(|e| e.to_string())?;
                }
                Ok::<u64, String>(id)
            })?;
        Ok((edit, vec![id, self.main]))
    }
}

#[cfg(test)]
mod tests {
    use super::super::testkit::*;
    use super::*;

    fn entry(p: &Project, main: u64) -> GraceEntry {
        GraceEntry::new(p, main).unwrap()
    }

    #[test]
    fn keys_change_pitch_string_fret_and_flags() {
        let mut p = project();
        let main = add(&mut p, 64, 1.0, 2.0);
        let mut e = entry(&p, main);
        assert_eq!((e.midi, e.kind, e.hammer), (62, GraceKind::Acciaccatura, false));
        e.key(&p, EntryKey::Pitch { up: true, tone: false });
        e.key(&p, EntryKey::Pitch { up: false, tone: true });
        assert_eq!(e.midi, 61);
        e.key(&p, EntryKey::Pitch { up: true, tone: false });
        assert_eq!(e.placement(&p), Some((4, 3)));
        e.key(&p, EntryKey::String { up: false });
        assert_eq!(e.placement(&p), Some((3, 7)));
        e.key(&p, EntryKey::Digit(5));
        assert_eq!((e.midi, e.placement(&p)), (60, Some((3, 5))));
        e.key(&p, EntryKey::Digit(2));
        assert_eq!(e.midi, 57, "52 is no fret: the digit starts over");
        e.key(&p, EntryKey::Pitch { up: true, tone: false });
        e.key(&p, EntryKey::Digit(1));
        e.key(&p, EntryKey::Digit(2));
        assert_eq!((e.midi, e.placement(&p)), (67, Some((3, 12))));
        e.key(&p, EntryKey::String { up: true });
        assert_eq!(e.placement(&p), Some((4, 8)));
        e.key(&p, EntryKey::ToggleKind);
        e.key(&p, EntryKey::ToggleHammer);
        assert_eq!((e.kind, e.hammer), (GraceKind::Appoggiatura, true));
        assert_eq!(e.key(&p, EntryKey::Confirm), EntryStep::Done);
    }

    #[test]
    fn preview_shows_the_grace_note_and_the_hammer() {
        let mut p = project();
        let main = add(&mut p, 64, 1.0, 2.0);
        let mut e = entry(&p, main);
        let (ghost, fb) = e.preview(&p).unwrap();
        assert_eq!((ghost.glyphs.len(), ghost.glyphs[0].grace, ghost.glyphs[1].legato_from), (2, true, None));
        assert_eq!(fb.labels[0].2, "B 3");
        e.key(&p, EntryKey::ToggleHammer);
        assert_eq!(e.preview(&p).unwrap().0.glyphs[1].legato_from, Some(0));
        assert_eq!(p.notes.len(), 1);
    }

    #[test]
    fn commit_is_one_undo_step_with_lock_and_hammer() {
        let mut p = project();
        let main = add(&mut p, 64, 1.0, 2.0);
        let before = p.notes.clone();
        let mut e = entry(&p, main);
        e.key(&p, EntryKey::String { up: false });
        e.key(&p, EntryKey::ToggleHammer);
        let (edit, affected) = e.commit(&mut p).unwrap();
        let grace = p.notes.iter().find(|n| n.grace_of.is_some()).unwrap();
        assert_eq!((grace.midi, grace.string_fret, grace.locked_fret), (62, Some((3, 7)), true));
        assert_eq!(p.note(main).unwrap().legato.map(|l| l.from), Some(grace.id));
        assert_eq!(affected, [grace.id, main]);
        edit.undo(&mut p);
        assert_eq!(p.notes, before);
    }

    #[test]
    fn a_failing_hammer_leaves_the_project_untouched() {
        let mut p = project();
        let main = add(&mut p, 64, 1.0, 2.0);
        let before = p.notes.clone();
        let mut e = entry(&p, main);
        e.midi = 64;
        e.key(&p, EntryKey::ToggleHammer);
        assert!(e.commit(&mut p).is_err());
        assert_eq!(p.notes, before);
    }
}
