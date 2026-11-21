//! Notes created by gestures: drawing, chain continuation and legato drags, with live feedback.

use fretscribe_core::{add_beats, infer_link, same_string_fit, snap_edge, snap_note, min_subdivision_beats, EditError, Edge, LinkType, Note, Project, ProjectEdit, TempoMap, TimePos};
use fretscribe_notation::tab::{candidate_label, lowest_fret_candidate};

use super::EditCtx;
use crate::spectro::{GhostGlyph, GhostShape, LinkKind};

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

/// Length of the finest grid division that has a line at `pos`: the shortest note started there.
fn min_len_at(ctx: &EditCtx, pos: TimePos) -> f64 {
    let on = |d: u8| (pos.beat() * f64::from(d) - (pos.beat() * f64::from(d)).round()).abs() < 1e-6;
    ctx.grid.divisions.iter().copied().filter(|&d| d > 0 && on(d)).max().map_or_else(|| min_subdivision_beats(ctx.grid), |d| 1.0 / f64::from(d))
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
    let (s, e, min) = if ctx.grid.divisions.is_empty() || ctx.alt {
        let (s, e) = snap_note(raw_start, raw_end, ctx.tempo, &ctx.project.notes, &opts);
        let free_end = snap_edge(raw_end, Edge::End, ctx.tempo, &ctx.project.notes, &opts);
        let min = min_subdivision_beats(ctx.grid);
        if beats(ctx, free_end) - beats(ctx, s) < min - EPS {
            return None;
        }
        (s, e, min)
    } else {
        // The press always lands on the closest grid line, whatever the zoom; the other end
        // follows the usual snapping but the note is never shorter than that line's division.
        let anchor = ctx.tempo.snap_to_pos(press.sec, ctx.grid);
        let min = min_len_at(ctx, anchor);
        if beats(ctx, raw_end) - beats(ctx, raw_start) < min / 2.0 {
            return None;
        }
        if forward {
            let end = snap_edge(raw_end, Edge::End, ctx.tempo, &ctx.project.notes, &opts);
            let floor = add_beats(ctx.tempo, anchor, min);
            (anchor, if beats(ctx, end) < beats(ctx, floor) - EPS { floor } else { end }, min)
        } else {
            let start = snap_edge(raw_start, Edge::Start, ctx.tempo, &ctx.project.notes, &opts);
            let floor = add_beats(ctx.tempo, anchor, -min);
            (if beats(ctx, start) > beats(ctx, floor) + EPS { floor } else { start }, anchor, min)
        }
    };
    let creation = if slide {
        Creation {
            notes: vec![
                NewNote { midi: p_first, start: s, end: e },
                NewNote { midi: p_second, start: e, end: add_beats(ctx.tempo, e, min) },
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
/// a slide when no string fits, or `forced`) joined to its right half's end or left half's start; past either
/// end a slide starting at the pointer. The pointer pitch is the new note's pitch.
pub fn plan_aim(src: u64, cur: Pt, target: Option<u64>, sticky: Sticky, forced: Option<LinkType>, ctx: &EditCtx) -> LinkAim {
    let project = ctx.project;
    let Some(source) = project.note(src) else {
        return LinkAim::Hidden;
    };
    if let Some(t) = target.and_then(|id| project.note(id)).filter(|t| t.id != src) {
        let (from, to) = if ctx.tempo.pos_to_sec(t.start) >= ctx.tempo.pos_to_sec(source.start) { (source, t) } else { (t, source) };
        let kind = forced.filter(|k| project.can_link(from.id, to.id, *k).is_ok()).unwrap_or_else(|| project.infer_link_type(from.id, to.id));
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
    let kind = forced.filter(|k| kind_fits(project, source.midi, midi, *k)).unwrap_or(if inside { infer_link(&project.tuning, project.capo, source.midi, midi) } else { LinkType::Slide });
    LinkAim::Start { start: LinkStart { src, forward, kind, midi, near }, snapped }
}

/// Whether a link of `kind` suits the pitches of a link to a new note (existing pairs go through `can_link`).
fn kind_fits(project: &Project, a: u8, b: u8, kind: LinkType) -> bool {
    let fit = |midis: &[u8]| same_string_fit(&project.tuning, project.capo, midis);
    match kind {
        LinkType::Slide => true,
        LinkType::Bend => fit(&[a, b]) && fit(&[a.min(b)]) && a.abs_diff(b) <= SOFT_BEND_LIMIT,
        LinkType::Legato | LinkType::Tied => fit(&[a, b]),
        LinkType::Dashed => false,
    }
}

/// The kinds (slide, bend, legato) the first link of `creation` can take, for L to cycle through.
pub fn valid_kinds(creation: &Creation, project: &Project) -> Vec<LinkType> {
    let Some(&(a, b, _)) = creation.links.first() else { return Vec::new() };
    let kinds = [LinkType::Slide, LinkType::Bend, LinkType::Legato];
    let midi = |r: Ref| match r {
        Ref::New(i) => creation.notes.get(i).map(|n| n.midi),
        Ref::Existing(id) => project.note(id).map(|n| n.midi),
    };
    match (a, b, midi(a), midi(b)) {
        (Ref::Existing(x), Ref::Existing(y), ..) => kinds.into_iter().filter(|k| project.can_link(x, y, *k).is_ok()).collect(),
        (_, _, Some(x), Some(y)) => kinds.into_iter().filter(|k| kind_fits(project, x, y, *k)).collect(),
        _ => Vec::new(),
    }
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

/// A drawn slide whose end is an existing note: the first new note links to it (when valid)
/// instead of a second new note being made.
pub(crate) fn plan_slide_target(base: &Creation, target: u64, ctx: &EditCtx) -> Option<Creation> {
    let first = base.notes.first()?;
    let mut scratch = ctx.project.clone();
    let id = fretscribe_core::next_note_id(&scratch);
    scratch.notes.push(Note::new(id, first.midi, first.start, first.end));
    scratch.can_link(id, target, LinkType::Slide).ok()?;
    Some(Creation { notes: vec![first.clone()], links: vec![(Ref::New(0), Ref::Existing(target), LinkType::Slide)] })
}

/// A left drag from empty space released on an existing note: the press is one end of the new
/// note and the target gives the other, so the link completes it. Target later than the press: the
/// new note runs from the press to the start of the target; target earlier: from the end of the
/// target to the press.
pub(crate) fn plan_draw_onto(press: Pt, target: u64, ctx: &EditCtx) -> Option<Creation> {
    let t = ctx.project.note(target)?;
    let at = snap_edge(ctx.tempo.sec_to_pos(press.sec), Edge::Start, ctx.tempo, &ctx.project.notes, &ctx.opts((None, None)));
    let midi = row(press.midi);
    let creation = if beats(ctx, at) < beats(ctx, t.start) - EPS {
        Creation { notes: vec![NewNote { midi, start: at, end: t.start }], links: vec![(Ref::New(0), Ref::Existing(target), LinkType::Slide)] }
    } else {
        Creation { notes: vec![NewNote { midi, start: t.end, end: at }], links: vec![(Ref::Existing(target), Ref::New(0), LinkType::Slide)] }
    };
    commit_creation(&mut ctx.project.clone(), &creation, "").ok()?;
    Some(creation)
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
        .map(|n| GhostGlyph { midi: n.midi, start_sec: tempo.pos_to_sec(n.start), end_sec: tempo.pos_to_sec(n.end), link_to: None, legato_from: None, grace: false })
        .collect();
    let index_of = |r: Ref, glyphs: &mut Vec<GhostGlyph>| match r {
        Ref::New(i) => i,
        Ref::Existing(id) => {
            if let Some(n) = project.note(id) {
                glyphs.push(GhostGlyph { midi: n.midi, start_sec: tempo.pos_to_sec(n.start), end_sec: tempo.pos_to_sec(n.end), link_to: None, legato_from: None, grace: false });
            }
            glyphs.len() - 1
        }
    };
    for &(a, b, kind) in &creation.links {
        let (ia, ib) = (index_of(a, &mut glyphs), index_of(b, &mut glyphs));
        match kind {
            LinkType::Legato | LinkType::Tied => glyphs[ib].legato_from = Some(ia),
            LinkType::Bend => glyphs[ia].link_to = Some((ib, LinkKind::Bend)),
            LinkType::Slide => glyphs[ia].link_to = Some((ib, LinkKind::Slide)),
            LinkType::Dashed => glyphs[ia].link_to = Some((ib, LinkKind::Dashed)),
        }
        glyphs[ia].end_sec = glyphs[ib].start_sec;
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

#[cfg(all(test, dev_tests))]
#[path = "../../../../dev/tests/unit/ui/note_editor/draw.rs"]
mod tests;
