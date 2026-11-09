//! Quick entry of a grace note: modal keys over a ghost note, one undo step on Enter.

use fretscribe_core::{GraceKind, GraceLink, Project, ProjectEdit, MAX_FRET};
use fretscribe_notation::tab::{candidate_label, lowest_fret_candidate};

use super::draw::DrawFeedback;
use crate::spectro::{GhostGlyph, GhostShape, LinkKind};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EntryKey {
    /// Semitone, or a tone with `tone`.
    Pitch { up: bool, tone: bool },
    /// Neighbouring string (up = towards the higher strings).
    String { up: bool },
    Digit(u8),
    ToggleKind,
    /// Next valid link: legato, slide, dashed.
    CycleLink,
    Confirm,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EntryStep {
    Continue,
    Done,
}

#[derive(Clone, Debug, PartialEq)]
pub struct GraceEntry {
    pub main: u64,
    pub midi: u8,
    pub string: Option<u8>,
    pub kind: GraceKind,
    /// `None` takes the default (legato when close, slide when further, dashed otherwise).
    pub link: Option<GraceLink>,
    /// Start under the pointer; `None` puts the grace note right before its main note.
    pub start_sec: Option<f64>,
    typed: Option<u8>,
}

impl GraceEntry {
    pub fn new(project: &Project, main: u64) -> Option<Self> {
        let midi = fretscribe_core::default_grace_midi(project.note(main)?.midi);
        Some(Self { main, midi, string: None, kind: GraceKind::Acciaccatura, link: None, start_sec: None, typed: None })
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
            EntryKey::CycleLink => {
                let mut scratch = project.clone();
                if let Ok(id) = self.place(&mut scratch) {
                    let valid = scratch.valid_grace_links(id);
                    let at = valid.iter().position(|&k| k == scratch.grace_link(id)).unwrap_or(0);
                    self.link = valid.get((at + 1) % valid.len().max(1)).copied();
                }
            }
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

    /// The pointer sets the start and the pitch (and drops string and fret choices).
    pub fn pointer(&mut self, sec: f64, midi: f32) {
        self.start_sec = Some(sec);
        self.midi = midi.round().clamp(0.0, 127.0) as u8;
        (self.string, self.typed) = (None, None);
    }

    /// Adds the grace note to `p` with the chosen start and link; an explicit link that is not
    /// valid at that place is ignored.
    fn place(&self, p: &mut Project) -> Result<u64, String> {
        let id = p.add_grace_note(self.main, Some(self.midi), self.kind).map_err(|e| e.to_string())?;
        if let Some(sec) = self.start_sec {
            p.move_grace(id, sec).map_err(|e| e.to_string())?;
        }
        if let Some(link) = self.link {
            let _ = p.set_grace_link(id, link);
        }
        Ok(id)
    }

    /// The ghost (grace note then main note, hammered when asked) and the "B 5" label.
    pub fn preview(&self, project: &Project) -> Option<(GhostShape, DrawFeedback)> {
        let mut scratch = project.clone();
        let id = self.place(&mut scratch).ok()?;
        let tempo = fretscribe_core::TempoMap::new(&scratch.measures);
        let link = scratch.grace_link(id);
        let glyph = |id: u64, link_to, legato_from| scratch.note(id).map(|n| GhostGlyph { midi: n.midi, start_sec: tempo.pos_to_sec(n.start), end_sec: tempo.pos_to_sec(n.end), link_to, legato_from, grace: n.grace_of.is_some() });
        let to_main = match link {
            GraceLink::Slide => Some((1, LinkKind::Slide)),
            GraceLink::Dashed => Some((1, LinkKind::Dashed)),
            GraceLink::Legato => None,
        };
        let glyphs = vec![glyph(id, to_main, None)?, glyph(self.main, None, (link == GraceLink::Legato).then_some(0))?];
        let label = self.placement(project).and_then(|(s, f)| candidate_label(&project.tuning, s, f)).unwrap_or_else(|| "?".into());
        let feedback = DrawFeedback { labels: vec![(glyphs[0].start_sec, self.midi, label)], ..DrawFeedback::default() };
        Some((GhostShape { glyphs }, feedback))
    }

    /// Adds the grace note (string locked when one was chosen) with its link.
    pub fn commit(&self, project: &mut Project) -> Result<(ProjectEdit, Vec<u64>), String> {
        let placement = self.placement(project).filter(|_| self.string.is_some());
        let (edit, id) = project
            .try_edit("Add grace note", |p| {
                let id = self.place(p)?;
                if let Some(note) = p.notes.iter_mut().find(|n| n.id == id).filter(|_| placement.is_some()) {
                    note.string_fret = placement;
                    note.locked_fret = true;
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
        assert_eq!((e.midi, e.kind, e.link), (62, GraceKind::Acciaccatura, None));
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
        assert_eq!(e.kind, GraceKind::Appoggiatura);
        assert_eq!(e.key(&p, EntryKey::Confirm), EntryStep::Done);
    }

    #[test]
    fn the_link_defaults_by_distance_and_h_cycles_the_valid_ones() {
        let mut p = project();
        add(&mut p, 40, 0.0, 0.5);
        let main = add(&mut p, 64, 3.0, 4.0);
        let mut e = entry(&p, main);
        let link = |e: &GraceEntry, p: &Project| {
            let mut s = p.clone();
            let id = e.place(&mut s).unwrap();
            s.grace_link(id)
        };
        assert_eq!(link(&e, &p), GraceLink::Legato);
        e.pointer(1.0, 62.2);
        assert_eq!((e.start_sec, e.midi, link(&e, &p)), (Some(1.0), 62, GraceLink::Slide));
        e.key(&p, EntryKey::CycleLink);
        assert_eq!((e.link, link(&e, &p)), (Some(GraceLink::Dashed), GraceLink::Dashed));
        e.key(&p, EntryKey::CycleLink);
        assert_eq!(e.link, Some(GraceLink::Slide));
        e.pointer(1.49, 62.0);
        e.link = None;
        assert_eq!(link(&e, &p), GraceLink::Legato);
    }

    #[test]
    fn preview_shows_the_grace_note_and_its_link() {
        let mut p = project();
        let main = add(&mut p, 64, 1.0, 2.0);
        let mut e = entry(&p, main);
        let (ghost, fb) = e.preview(&p).unwrap();
        assert_eq!((ghost.glyphs.len(), ghost.glyphs[0].grace, ghost.glyphs[1].legato_from), (2, true, Some(0)));
        assert_eq!(fb.labels[0].2, "B 3");
        e.key(&p, EntryKey::CycleLink);
        assert_eq!(e.preview(&p).unwrap().0.glyphs[0].link_to, Some((1, LinkKind::Slide)));
        e.key(&p, EntryKey::CycleLink);
        assert_eq!(e.preview(&p).unwrap().0.glyphs[0].link_to, Some((1, LinkKind::Dashed)));
        assert_eq!(p.notes.len(), 1);
    }

    #[test]
    fn commit_is_one_undo_step_with_start_lock_and_link() {
        let mut p = project();
        let main = add(&mut p, 64, 1.0, 2.0);
        let before = p.notes.clone();
        let mut e = entry(&p, main);
        e.key(&p, EntryKey::String { up: false });
        e.pointer(0.2, 60.0);
        e.key(&p, EntryKey::String { up: false });
        let (edit, affected) = e.commit(&mut p).unwrap();
        let grace = p.notes.iter().find(|n| n.grace_of.is_some()).unwrap();
        assert_eq!((grace.midi, grace.string_fret, grace.locked_fret), (60, Some((3, 5)), true));
        assert_eq!(p.grace_link(grace.id), GraceLink::Slide);
        assert_eq!(affected, [grace.id, main]);
        edit.undo(&mut p);
        assert_eq!(p.notes, before);
    }

    #[test]
    fn the_default_legato_is_made_in_the_same_step() {
        let mut p = project();
        let main = add(&mut p, 64, 1.0, 2.0);
        let (edit, _) = entry(&p, main).commit(&mut p).unwrap();
        let grace = p.notes.iter().find(|n| n.grace_of.is_some()).unwrap().id;
        assert_eq!(p.note(main).unwrap().legato.map(|l| l.from), Some(grace));
        edit.undo(&mut p);
        assert_eq!(p.notes.len(), 1);
    }
}
