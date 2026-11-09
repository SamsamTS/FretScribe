//! Notes created by gestures: drawing, chain continuation and legato drags, with live feedback.

use fretscribe_core::{add_beats, infer_link, same_string_fit, snap_edge, snap_note, min_subdivision_beats, EditError, Edge, LinkType, Note, Project, ProjectEdit, TempoMap, TimePos};
use fretscribe_notation::tab::{candidate_label, lowest_fret_candidate};

use super::EditCtx;
use crate::spectro::{GhostGlyph, GhostShape, LinkKind};

const SLIDE_START_SHARE: f64 = 0.4;
const SLIDE_ENTER_SEMITONES: f32 = 0.75;
const SLIDE_LEAVE_SEMITONES: f32 = 0.5;
const SOFT_BEND_LIMIT: u8 = 4;
const EPS: f64 = 1e-9;

/// Pointer sample: time in project seconds and the (refined) fractional pitch.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Pt {
    pub sec: f64,
    pub midi: f32,
}

#[derive(Clone, Debug, PartialEq)]
pub struct NewNote {
    pub midi: u8,
    pub start: TimePos,
    pub end: TimePos,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Ref {
    New(usize),
    Existing(u64),
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Creation {
    pub notes: Vec<NewNote>,
    pub links: Vec<(Ref, Ref, LinkType)>,
}

/// Snapped edges of the previous plan, fed back for hysteresis.
pub type Sticky = (Option<TimePos>, Option<TimePos>);

#[derive(Clone, Debug, PartialEq)]
pub struct DrawPlan {
    pub creation: Creation,
    pub snapped: Sticky,
    pub slide: bool,
}

fn row(midi: f32) -> u8 {
    midi.round().clamp(0.0, 127.0) as u8
}

fn beats(ctx: &EditCtx, pos: TimePos) -> f64 {
    ctx.tempo.pos_to_beats(pos).unwrap_or(0.0)
}

fn step_beats(ctx: &EditCtx) -> f64 {
    let finest = ctx.grid.divisions.iter().copied().max().unwrap_or(1);
    (1.0 / f64::from(finest)).max(min_subdivision_beats(ctx.grid))
}

/// The press is one end and the pointer the other; the earlier in time is the start. A pitch more
/// than 0.75 semitone from the press row (0.5 once away) makes it a slide of two linked notes.
pub fn plan_draw(press: Pt, cur: Pt, was_slide: bool, sticky: Sticky, ctx: &EditCtx) -> Option<DrawPlan> {
    let forward = press.sec <= cur.sec;
    let (first, second) = if forward { (press, cur) } else { (cur, press) };
    let press_row = row(press.midi);
    let limit = if was_slide { SLIDE_LEAVE_SEMITONES } else { SLIDE_ENTER_SEMITONES };
    let slide = (cur.midi - f32::from(press_row)).abs() > limit;
    let cur_row = if slide { row(cur.midi) } else { press_row };
    let (p_first, p_second) = if forward { (press_row, cur_row) } else { (cur_row, press_row) };

    let (raw_start, raw_end) = (ctx.tempo.sec_to_pos(first.sec), ctx.tempo.sec_to_pos(second.sec));
    let opts = ctx.opts(sticky);
    let (s, e) = snap_note(raw_start, raw_end, ctx.tempo, &ctx.project.notes, &opts);
    let free_end = snap_edge(raw_end, Edge::End, ctx.tempo, &ctx.project.notes, &opts);
    let min = min_subdivision_beats(ctx.grid);
    if beats(ctx, free_end) - beats(ctx, s) < min - EPS {
        return None;
    }
    let span = beats(ctx, e) - beats(ctx, s);
    let creation = if slide {
        let units = ((SLIDE_START_SHARE * span).min(step_beats(ctx)) / min + EPS).floor().max(1.0);
        let start_len = (units * min).min(span);
        Creation {
            notes: vec![
                NewNote { midi: p_first, start: s, end: add_beats(ctx.tempo, s, start_len) },
                NewNote { midi: p_second, start: e, end: add_beats(ctx.tempo, e, step_beats(ctx)) },
            ],
            links: vec![(Ref::New(0), Ref::New(1), LinkType::Slide)],
        }
    } else {
        Creation { notes: vec![NewNote { midi: p_first, start: s, end: e }], links: Vec::new() }
    };
    Some(DrawPlan { creation, snapped: (Some(s), Some(e)), slide })
}

/// Shift + drag from the last note of a chain: one more note at the pointer, linked by a slide (a
/// bend when the chain's last link is one).
pub fn plan_chain(last: u64, cur: Pt, sticky: Sticky, ctx: &EditCtx) -> Option<DrawPlan> {
    let project = ctx.project;
    let from = project.note(last)?;
    if project.outgoing(last).is_some() {
        return None;
    }
    let kind = match project.incoming(last) {
        Some((_, LinkType::Bend)) => LinkType::Bend,
        _ => LinkType::Slide,
    };
    let midi = row(cur.midi);
    let interval = midi.abs_diff(from.midi);
    if interval == 0 || (kind == LinkType::Bend && interval > SOFT_BEND_LIMIT) {
        return None;
    }
    let start = snap_edge(ctx.tempo.sec_to_pos(cur.sec), Edge::Start, ctx.tempo, &project.notes, &ctx.opts(sticky));
    if beats(ctx, start) < beats(ctx, from.start) - EPS {
        return None;
    }
    let end = add_beats(ctx.tempo, start, step_beats(ctx));
    Some(DrawPlan {
        creation: Creation { notes: vec![NewNote { midi, start, end }], links: vec![(Ref::Existing(last), Ref::New(0), kind)] },
        snapped: (Some(start), Some(end)),
        slide: true,
    })
}

/// First point of a right-drag link, fixed on release: the new note's near end (its start when
/// `forward`) and pitch. The link kind is the one previewed.
#[derive(Clone, Debug, PartialEq)]
pub struct LinkStart {
    pub src: u64,
    pub forward: bool,
    pub kind: LinkType,
    pub midi: u8,
    pub near: TimePos,
}

#[derive(Clone, Debug, PartialEq)]
pub enum LinkAim {
    Hidden,
    /// Link of two existing notes, complete.
    Join(Creation),
    Start { start: LinkStart, snapped: Sticky },
}

/// Right-drag from a note, first step. Over another note: a link to it when valid (nothing when
/// not). Elsewhere, inside the span of the source a legato (hammer-on or pull-off, falling back to
/// a slide when no string fits) joined to its right half's end or left half's start; past either
/// end a slide starting at the pointer. The pointer pitch is the new note's pitch.
pub fn plan_aim(src: u64, cur: Pt, target: Option<u64>, sticky: Sticky, ctx: &EditCtx) -> LinkAim {
    let project = ctx.project;
    let Some(source) = project.note(src) else {
        return LinkAim::Hidden;
    };
    if let Some(t) = target.and_then(|id| project.note(id)).filter(|t| t.id != src) {
        let (from, to) = if ctx.tempo.pos_to_sec(t.start) >= ctx.tempo.pos_to_sec(source.start) { (source, t) } else { (t, source) };
        let kind = project.infer_link_type(from.id, to.id);
        return if project.can_link(from.id, to.id, kind).is_ok() {
            LinkAim::Join(Creation { notes: Vec::new(), links: vec![(Ref::Existing(from.id), Ref::Existing(to.id), kind)] })
        } else {
            LinkAim::Hidden
        };
    }
    let (s0, s1) = (ctx.tempo.pos_to_sec(source.start), ctx.tempo.pos_to_sec(source.end));
    let inside = cur.sec >= s0 && cur.sec <= s1;
    let forward = if inside { cur.sec >= (s0 + s1) / 2.0 } else { cur.sec > s1 };
    let midi = row(cur.midi);
    let busy = if forward { project.outgoing(src) } else { project.incoming(src) };
    if midi == source.midi || busy.is_some() {
        return LinkAim::Hidden;
    }
    let bound = if forward { source.end } else { source.start };
    let (near, snapped) = if inside {
        (bound, (None, None))
    } else {
        let at = snap_edge(ctx.tempo.sec_to_pos(cur.sec), if forward { Edge::Start } else { Edge::End }, ctx.tempo, &project.notes, &ctx.opts(sticky));
        let wrong = if forward { beats(ctx, at) < beats(ctx, bound) } else { beats(ctx, at) > beats(ctx, bound) };
        let near = if wrong { bound } else { at };
        (near, if forward { (Some(near), None) } else { (None, Some(near)) })
    };
    let kind = if inside { infer_link(&project.tuning, project.capo, source.midi, midi) } else { LinkType::Slide };
    LinkAim::Start { start: LinkStart { src, forward, kind, midi, near }, snapped }
}

/// Right-drag link, second step: the new note runs from the fixed point to the pointer (one step
/// long while the pointer is not past it, or `None`).
pub fn plan_extend(start: &LinkStart, cur_sec: Option<f64>, sticky: Sticky, ctx: &EditCtx) -> DrawPlan {
    let step = step_beats(ctx);
    let minimal = add_beats(ctx.tempo, start.near, if start.forward { step } else { -step });
    let free = cur_sec.map(|sec| snap_edge(ctx.tempo.sec_to_pos(sec), if start.forward { Edge::End } else { Edge::Start }, ctx.tempo, &ctx.project.notes, &ctx.opts(sticky)));
    let long_enough = |far: TimePos| {
        let len = if start.forward { beats(ctx, far) - beats(ctx, start.near) } else { beats(ctx, start.near) - beats(ctx, far) };
        len >= min_subdivision_beats(ctx.grid) - EPS
    };
    let (far, snapped) = match free.filter(|f| long_enough(*f)) {
        Some(far) => (far, if start.forward { (None, Some(far)) } else { (Some(far), None) }),
        None => (minimal, (None, None)),
    };
    let (first, second) = if start.forward { (start.near, far) } else { (far, start.near) };
    let new = Ref::New(0);
    let link = if start.forward { (Ref::Existing(start.src), new, start.kind) } else { (new, Ref::Existing(start.src), start.kind) };
    DrawPlan { creation: Creation { notes: vec![NewNote { midi: start.midi, start: first, end: second }], links: vec![link] }, snapped, slide: start.kind == LinkType::Slide }
}

/// Third step of a drawn slide: the last note of `base` ends at the pointer (it keeps its minimal
/// length while the pointer is not past its start). Returns the creation and the end snap.
pub fn plan_slide_end(base: &Creation, cur_sec: f64, sticky: Sticky, ctx: &EditCtx) -> (Creation, Sticky) {
    let mut creation = base.clone();
    let Some(last) = creation.notes.last_mut() else {
        return (creation, sticky);
    };
    let free = snap_edge(ctx.tempo.sec_to_pos(cur_sec), Edge::End, ctx.tempo, &ctx.project.notes, &ctx.opts(sticky));
    if beats(ctx, free) - beats(ctx, last.start) < min_subdivision_beats(ctx.grid) - EPS {
        return (creation, (None, None));
    }
    last.end = free;
    (creation, (None, Some(free)))
}

/// Adds the notes and links of `creation` as one edit; returns the ids of the new notes.
pub fn commit_creation(project: &mut Project, creation: &Creation, label: &str) -> Result<(ProjectEdit, Vec<u64>), EditError> {
    let mut ids = Vec::new();
    let edit = project.edit_notes(label, |p| {
        for (id, n) in (fretscribe_core::next_note_id(p)..).zip(&creation.notes) {
            p.notes.push(Note::new(id, n.midi, n.start, n.end));
            ids.push(id);
        }
        let resolve = |r: Ref| match r {
            Ref::New(i) => ids.get(i).copied(),
            Ref::Existing(id) => Some(id),
        };
        for &(a, b, kind) in &creation.links {
            let (Some(a), Some(b)) = (resolve(a), resolve(b)) else {
                return Err(EditError::UnknownNote(0));
            };
            p.link_notes(a, b, kind)?;
        }
        p.resolve_overlaps(&ids);
        Ok(())
    })?;
    Ok((edit, ids))
}

/// Dashed preview; an existing note a link starts or ends on is repeated as a ghost.
pub fn ghost_of(creation: &Creation, project: &Project, tempo: &TempoMap) -> GhostShape {
    let mut glyphs: Vec<GhostGlyph> = creation
        .notes
        .iter()
        .map(|n| GhostGlyph { midi: n.midi, start_sec: tempo.pos_to_sec(n.start), end_sec: tempo.pos_to_sec(n.end), link_to: None, legato_from: None })
        .collect();
    let index_of = |r: Ref, glyphs: &mut Vec<GhostGlyph>| match r {
        Ref::New(i) => i,
        Ref::Existing(id) => {
            if let Some(n) = project.note(id) {
                glyphs.push(GhostGlyph { midi: n.midi, start_sec: tempo.pos_to_sec(n.start), end_sec: tempo.pos_to_sec(n.end), link_to: None, legato_from: None });
            }
            glyphs.len() - 1
        }
    };
    for &(a, b, kind) in &creation.links {
        let (ia, ib) = (index_of(a, &mut glyphs), index_of(b, &mut glyphs));
        match kind {
            LinkType::Legato => glyphs[ib].legato_from = Some(ia),
            LinkType::Bend => glyphs[ia].link_to = Some((ib, LinkKind::Bend)),
            LinkType::Slide => glyphs[ia].link_to = Some((ib, LinkKind::Slide)),
        }
    }
    GhostShape { glyphs }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct DrawFeedback {
    /// Start time, pitch and the "B 5" string/fret label of every new note.
    pub labels: Vec<(f64, u8, String)>,
    pub link: Option<LinkType>,
    pub interval: Option<String>,
    pub warning: Option<String>,
}

/// "+1 (full)", "+½", "-1": bend vocabulary, a tone being two semitones.
pub fn interval_label(semitones: i32) -> String {
    let sign = if semitones < 0 { '-' } else { '+' };
    let n = semitones.unsigned_abs();
    let amount = match (n / 2, n % 2) {
        (0, _) => "½".to_owned(),
        (tones, 0) => tones.to_string(),
        (tones, _) => format!("{tones}½"),
    };
    let full = if semitones == 2 { " (full)" } else { "" };
    format!("{sign}{amount}{full}")
}

pub fn feedback(creation: &Creation, project: &Project, tempo: &TempoMap) -> DrawFeedback {
    let label = |midi: u8| {
        lowest_fret_candidate(&project.tuning, project.capo, midi)
            .and_then(|(string, fret)| candidate_label(&project.tuning, string, fret))
            .unwrap_or_else(|| "?".to_owned())
    };
    let midi_of = |r: Ref| match r {
        Ref::New(i) => creation.notes.get(i).map(|n| n.midi),
        Ref::Existing(id) => project.note(id).map(|n| n.midi),
    };
    let mut out = DrawFeedback {
        labels: creation.notes.iter().map(|n| (tempo.pos_to_sec(n.start), n.midi, label(n.midi))).collect(),
        ..DrawFeedback::default()
    };
    if let Some(&(a, b, kind)) = creation.links.first() {
        if let (Some(from), Some(to)) = (midi_of(a), midi_of(b)) {
            out.link = Some(kind);
            out.interval = Some(interval_label(i32::from(to) - i32::from(from)));
            let lower = from.min(to);
            out.warning = if !same_string_fit(&project.tuning, project.capo, &[from, to]) {
                Some("No same-string placement".to_owned())
            } else if kind == LinkType::Bend && !same_string_fit(&project.tuning, project.capo, &[lower]) {
                Some("No placement for the bend".to_owned())
            } else if kind == LinkType::Bend && from.abs_diff(to) > SOFT_BEND_LIMIT {
                Some("Bend above 4 semitones".to_owned())
            } else {
                None
            };
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::super::testkit::*;
    use super::*;

    fn pt(beat: f64, midi: f32) -> Pt {
        Pt { sec: sec(beat), midi }
    }

    fn draw(p: &Project, press: Pt, cur: Pt, was_slide: bool, alt: bool) -> Option<DrawPlan> {
        with_ctx(p, &[4, 3], alt, |ctx| plan_draw(press, cur, was_slide, (None, None), ctx))
    }

    #[test]
    fn drawing_either_way_gives_the_same_note() {
        let p = project();
        let right = draw(&p, pt(1.0, 64.1), pt(2.02, 64.0), false, false).unwrap();
        let left = draw(&p, pt(2.02, 64.0), pt(1.0, 64.1), false, false).unwrap();
        assert_eq!(right.creation, left.creation);
        assert_eq!(right.creation.notes, [NewNote { midi: 64, start: at(1.0), end: at(2.0) }]);
        assert!(right.creation.links.is_empty());
    }

    #[test]
    fn a_different_pitch_makes_two_linked_slide_notes_with_pitches_following_their_points() {
        let p = project();
        let plan = draw(&p, pt(1.0, 60.0), pt(3.0, 62.1), false, false).unwrap();
        let c = plan.creation;
        assert_eq!(c.notes[0], NewNote { midi: 60, start: at(1.0), end: at(1.25) });
        assert_eq!(c.notes[1], NewNote { midi: 62, start: at(3.0), end: at(3.25) });
        assert_eq!(c.links, [(Ref::New(0), Ref::New(1), LinkType::Slide)]);
        let back = draw(&p, pt(3.0, 62.0), pt(1.0, 60.0), false, false).unwrap().creation;
        assert_eq!((back.notes[0].midi, back.notes[1].midi), (60, 62));
    }

    #[test]
    fn the_slide_start_note_never_exceeds_forty_percent_of_the_span_and_stays_on_the_lattice() {
        let p = project();
        near(draw(&p, pt(1.0, 60.0), pt(1.5, 62.0), false, false).unwrap().creation.notes[0].end, 1.0 + 2.0 / 12.0);
        near(draw(&p, pt(1.0, 60.0), pt(1.0 + 1.0 / 3.0, 62.0), false, false).unwrap().creation.notes[0].end, 1.0 + 1.0 / 12.0);
        near(draw(&p, pt(1.0, 60.0), pt(1.25, 62.0), false, false).unwrap().creation.notes[0].end, 1.0 + 1.0 / 12.0);
    }

    #[test]
    fn slide_hysteresis_enters_at_075_and_leaves_at_05() {
        let p = project();
        let notes = |midi: f32, was| draw(&p, pt(1.0, 60.0), pt(2.0, midi), was, false).unwrap().creation.notes.len();
        assert_eq!(notes(60.7, false), 1);
        assert_eq!(notes(60.8, false), 2);
        assert_eq!(notes(60.6, true), 2);
        assert_eq!(notes(60.4, true), 1);
        assert_eq!(notes(59.2, false), 2);
    }

    #[test]
    fn a_click_or_a_tiny_span_creates_nothing() {
        let p = project();
        assert!(draw(&p, pt(1.0, 60.0), pt(1.0, 60.0), false, false).is_none());
        assert!(draw(&p, pt(1.0, 60.0), pt(1.04, 60.0), false, false).is_none());
        assert!(draw(&p, pt(1.0, 60.0), pt(1.32, 60.0), false, false).is_some());
    }

    #[test]
    fn both_ends_snap_and_alt_disables_it() {
        let p = project();
        let plan = draw(&p, pt(1.03, 60.0), pt(2.27, 60.0), false, false).unwrap();
        assert_eq!(plan.creation.notes[0].start, at(1.0));
        assert_eq!(plan.creation.notes[0].end, at(2.25));
        let free = draw(&p, pt(1.03, 60.0), pt(2.27, 60.0), false, true).unwrap();
        assert!((free.creation.notes[0].start.beat - 1.03).abs() < 1e-9);
        assert!((free.creation.notes[0].end.beat - 2.27).abs() < 1e-9);
    }

    #[test]
    fn drawing_snaps_across_a_tempo_change() {
        let mut p = project();
        p.set_tempo_scoped(1, 60.0, fretscribe_core::Propagation::Single, fretscribe_core::NotePolicy::FollowGrid).unwrap();
        let (m1, m2) = (p.measures[0].id, p.measures[1].id);
        let tempo = TempoMap::new(&p.measures);
        let grid = fretscribe_core::SnapGrid::default();
        let ctx = EditCtx { project: &p, tempo: &tempo, grid: &grid, px_per_sec: PX_PER_SEC, threshold_px: 12.0, onsets: None, alt: false };
        let start = Pt { sec: tempo.pos_to_sec(TimePos::new(m1, 3.0)), midi: 60.0 };
        let end = Pt { sec: tempo.pos_to_sec(TimePos::new(m2, 1.02)), midi: 60.0 };
        let c = plan_draw(start, end, false, (None, None), &ctx).unwrap().creation;
        assert_eq!((c.notes[0].start, c.notes[0].end), (TimePos::new(m1, 3.0), TimePos::new(m2, 1.0)));
    }

    #[test]
    fn chain_extension_links_from_the_last_note_and_follows_a_bend() {
        let mut p = project();
        let a = add(&mut p, 60, 0.0, 1.0);
        let plan = with_ctx(&p, &[4, 3], false, |ctx| plan_chain(a, pt(2.0, 62.0), (None, None), ctx)).unwrap();
        assert_eq!(plan.creation.links, [(Ref::Existing(a), Ref::New(0), LinkType::Slide)]);
        assert_eq!(plan.creation.notes, [NewNote { midi: 62, start: at(2.0), end: at(2.25) }]);
        let (_, ids) = commit_creation(&mut p, &plan.creation, "Chain").unwrap();
        p.unlink(a);
        p.link_notes(a, ids[0], LinkType::Bend).unwrap();
        let again = with_ctx(&p, &[4, 3], false, |ctx| plan_chain(ids[0], pt(4.0, 64.0), (None, None), ctx)).unwrap();
        assert_eq!(again.creation.links[0].2, LinkType::Bend);
        assert!(with_ctx(&p, &[4, 3], false, |ctx| plan_chain(a, pt(4.0, 64.0), (None, None), ctx)).is_none());
        assert!(with_ctx(&p, &[4, 3], false, |ctx| plan_chain(ids[0], pt(4.0, 62.2), (None, None), ctx)).is_none());
    }

    #[test]
    fn committing_a_slide_is_one_undoable_edit_with_fresh_ids() {
        let mut p = project();
        let existing = add(&mut p, 50, 0.0, 1.0);
        let plan = draw(&p, pt(2.0, 60.0), pt(3.0, 62.0), false, false).unwrap();
        let before = p.notes.clone();
        let (edit, ids) = commit_creation(&mut p, &plan.creation, "Draw").unwrap();
        assert_eq!(ids, [existing + 1, existing + 2]);
        assert_eq!(p.outgoing(ids[0]), Some((ids[1], LinkType::Slide)));
        edit.undo(&mut p);
        assert_eq!(p.notes, before);
    }

    fn aim(p: &Project, src: u64, cur: Pt, on: Option<u64>) -> LinkAim {
        with_ctx(p, &[4, 3], false, |ctx| plan_aim(src, cur, on, (None, None), ctx))
    }

    fn start(p: &Project, src: u64, cur: Pt) -> LinkStart {
        let LinkAim::Start { start, .. } = aim(p, src, cur, None) else { panic!("start") };
        start
    }

    #[test]
    fn inside_the_source_is_a_legato_on_the_near_half_s_side() {
        let mut p = project();
        let s = add(&mut p, 60, 2.0, 3.0);
        let fwd = start(&p, s, pt(2.8, 62.0));
        assert_eq!((fwd.forward, fwd.kind, fwd.midi, fwd.near), (true, LinkType::Legato, 62, at(3.0)));
        let rev = start(&p, s, pt(2.2, 58.0));
        assert_eq!((rev.forward, rev.kind, rev.near), (false, LinkType::Legato, at(2.0)));
        let far = add(&mut p, 40, 5.0, 6.0);
        assert_eq!(start(&p, far, pt(5.8, 65.0)).kind, LinkType::Slide, "no string fits");
    }

    #[test]
    fn past_either_end_is_a_slide_starting_at_the_pointer() {
        let mut p = project();
        let s = add(&mut p, 60, 2.0, 3.0);
        let fwd = start(&p, s, pt(4.02, 62.0));
        assert_eq!((fwd.forward, fwd.kind, fwd.near), (true, LinkType::Slide, TimePos::new(2, 0.0)));
        let rev = start(&p, s, pt(1.0, 58.0));
        assert_eq!((rev.forward, rev.kind, rev.near), (false, LinkType::Slide, at(1.0)));
        let LinkAim::Start { snapped, .. } = aim(&p, s, pt(4.02, 62.0), None) else { panic!() };
        assert_eq!(snapped, (Some(TimePos::new(2, 0.0)), None));
    }

    #[test]
    fn aim_needs_a_different_pitch_and_a_free_side() {
        let mut p = project();
        let (s, t) = (add(&mut p, 60, 2.0, 3.0), add(&mut p, 62, 3.0, 4.0));
        assert_eq!(aim(&p, s, pt(4.0, 60.2), None), LinkAim::Hidden);
        p.link_notes(s, t, LinkType::Slide).unwrap();
        assert_eq!(aim(&p, s, pt(5.0, 65.0), None), LinkAim::Hidden);
        assert!(matches!(aim(&p, s, pt(1.0, 65.0), None), LinkAim::Start { .. }));
    }

    #[test]
    fn extending_runs_from_the_fixed_point_and_never_collapses() {
        let mut p = project();
        let s = add(&mut p, 60, 2.0, 3.0);
        let fwd = start(&p, s, pt(2.8, 62.0));
        let run = |st: &LinkStart, sec: Option<f64>| with_ctx(&p, &[4, 3], false, |ctx| plan_extend(st, sec, (None, None), ctx));
        let plan = run(&fwd, Some(sec(5.0)));
        assert_eq!(plan.creation.notes, [NewNote { midi: 62, start: at(3.0), end: TimePos::new(2, 1.0) }]);
        assert_eq!(plan.creation.links, [(Ref::Existing(s), Ref::New(0), LinkType::Legato)]);
        assert_eq!(plan.snapped, (None, Some(TimePos::new(2, 1.0))));
        assert_eq!(run(&fwd, None).creation.notes[0].end, at(3.25));
        assert_eq!(run(&fwd, Some(sec(2.0))).creation.notes[0].end, at(3.25));
        let rev = start(&p, s, pt(2.2, 58.0));
        let plan = run(&rev, Some(sec(0.5)));
        assert_eq!(plan.creation.notes, [NewNote { midi: 58, start: at(0.5), end: at(2.0) }]);
        assert_eq!(plan.creation.links, [(Ref::New(0), Ref::Existing(s), LinkType::Legato)]);
    }

    #[test]
    fn aim_over_a_note_joins_it_when_valid_in_either_direction() {
        let mut p = project();
        let (s, t) = (add(&mut p, 60, 0.0, 1.0), add(&mut p, 62, 1.05, 2.0));
        let LinkAim::Join(c) = aim(&p, s, pt(1.5, 62.0), Some(t)) else { panic!("join") };
        assert!(c.notes.is_empty());
        assert_eq!(c.links, [(Ref::Existing(s), Ref::Existing(t), LinkType::Legato)]);
        let LinkAim::Join(c) = aim(&p, t, pt(0.3, 60.0), Some(s)) else { panic!("join before") };
        assert_eq!(c.links, [(Ref::Existing(s), Ref::Existing(t), LinkType::Legato)]);
        let far = add(&mut p, 64, 3.0, 4.0);
        let LinkAim::Join(c) = aim(&p, s, pt(3.5, 64.0), Some(far)) else { panic!("far") };
        assert_eq!(c.links, [(Ref::Existing(s), Ref::Existing(far), LinkType::Slide)]);
        let same = add(&mut p, 60, 5.0, 6.0);
        assert_eq!(aim(&p, s, pt(5.5, 60.0), Some(same)), LinkAim::Hidden);
    }

    #[test]
    fn slide_end_follows_the_pointer_and_keeps_the_minimal_length_before_the_start() {
        let p = project();
        let plan = draw(&p, pt(0.0, 60.0), pt(2.0, 62.0), false, false).unwrap();
        let run = |sec| with_ctx(&p, &[4, 3], false, |ctx| plan_slide_end(&plan.creation, sec, (None, None), ctx));
        let (c, snapped) = run(sec(4.0));
        let end = TimePos::new(2, 0.0);
        assert_eq!((c.notes[1].start, c.notes[1].end, snapped.1), (at(2.0), end, Some(end)));
        assert_eq!(c.notes[0], plan.creation.notes[0]);
        assert_eq!(run(sec(1.0)).0, plan.creation);
    }

    #[test]
    fn intervals_use_bend_vocabulary() {
        assert_eq!(interval_label(2), "+1 (full)");
        assert_eq!(interval_label(1), "+½");
        assert_eq!(interval_label(-2), "-1");
        assert_eq!(interval_label(-1), "-½");
        assert_eq!(interval_label(3), "+1½");
        assert_eq!(interval_label(4), "+2");
    }

    #[test]
    fn feedback_labels_ends_and_warns_without_a_same_string_placement() {
        let mut p = project();
        let _ = add(&mut p, 50, 0.0, 1.0);
        let ok = draw(&p, pt(2.0, 64.0), pt(3.0, 66.0), false, false).unwrap().creation;
        let tempo = TempoMap::new(&p.measures);
        let fb = feedback(&ok, &p, &tempo);
        assert_eq!(fb.labels.iter().map(|l| l.2.as_str()).collect::<Vec<_>>(), ["E 0", "E 2"]);
        assert_eq!(fb.interval.as_deref(), Some("+1 (full)"));
        assert_eq!(fb.warning, None);
        let bad = draw(&p, pt(2.0, 40.0), pt(3.0, 65.0), false, false).unwrap().creation;
        assert_eq!(feedback(&bad, &p, &tempo).warning.as_deref(), Some("No same-string placement"));
    }

    #[test]
    fn ghost_links_new_notes_by_index_and_repeats_an_existing_source() {
        let mut p = project();
        let s = add(&mut p, 60, 0.0, 1.0);
        let tempo = TempoMap::new(&p.measures);
        let plan = draw(&p, pt(2.0, 60.0), pt(3.0, 62.0), false, false).unwrap();
        let ghost = ghost_of(&plan.creation, &p, &tempo);
        assert_eq!(ghost.glyphs.len(), 2);
        assert_eq!(ghost.glyphs[0].link_to, Some((1, LinkKind::Slide)));
        let d = with_ctx(&p, &[4, 3], false, |ctx| plan_extend(&start(&p, s, pt(0.8, 62.0)), None, (None, None), ctx));
        let ghost = ghost_of(&d.creation, &p, &tempo);
        assert_eq!(ghost.glyphs.len(), 2);
        assert_eq!(ghost.glyphs[0].legato_from, Some(1));
    }
}
