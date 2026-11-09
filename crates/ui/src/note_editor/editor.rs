//! The gesture state machine: pointer samples, `Draw*` events, keys and actions in; effects out.
//! Edits are applied to the project right away and handed back as undo steps.

use egui::Pos2;
use fretscribe_core::{snap_edge, Edge, Project, ProjectEdit, SnapGrid, TempoMap};

use super::action::{apply, ActionError, NoteAction};
use super::clip::Clipboard;
use super::grace_entry::{EntryKey, EntryStep, GraceEntry};
use super::draw::{commit_creation, feedback, ghost_of, plan_aim, plan_chain, plan_draw, plan_extend, plan_slide_end, Creation, DrawFeedback, LinkAim, LinkStart, Pt, Sticky};
use super::modify::{NoteChange, apply_changes, apply_resize, ghost_of_changes, is_noop, plan_move, plan_resize, preview_changes};
use super::select::{marquee_hits, time_order, Selection};
use super::EditCtx;
use crate::spectro::{GhostShape, HitZone, SpectroEvent};

const DRAG_PX: f32 = 4.0;
const CYCLE_WINDOW_SEC: f64 = 1.0;
const NO_STICKY: Sticky = (None, None);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Button {
    Left,
    Right,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Mods {
    pub shift: bool,
    pub ctrl: bool,
    pub alt: bool,
}

/// The pointer on the source lane: screen position, project time, raw and refined pitch, and what
/// `hit_test` found under it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Sample {
    pub pos: Pos2,
    pub sec: f64,
    pub midi: f32,
    pub refined: f32,
    pub hit: Option<(u64, HitZone)>,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Input {
    Press { button: Button, at: Sample, mods: Mods },
    Move { at: Sample, mods: Mods },
    Release { button: Button, at: Sample, mods: Mods },
    /// From the spectrogram: seek on a click, `Draw*` for a left drag from empty space. The pitches
    /// of `Draw*` are expected to be refined already.
    Spectro(SpectroEvent, Mods),
    Escape,
    Action(NoteAction),
    /// A key of the grace entry (only meaningful while `in_entry`).
    Entry(EntryKey),
    Copy,
    Paste,
}

#[derive(Debug)]
pub enum Effect {
    Seek(f64),
    /// `merge` folds this step into the previous one (consecutive link-type cycles).
    Edit { edit: Box<ProjectEdit>, merge: bool, affected: Vec<u64> },
    Message(String),
    OpenExact(u64),
}

pub struct Env<'a> {
    pub project: &'a mut Project,
    pub grid: &'a SnapGrid,
    pub px_per_sec: f64,
    pub threshold_px: f64,
    pub onsets: Option<&'a [f64]>,
    /// Seconds, any monotonic clock.
    pub now: f64,
    pub hovered: Option<u64>,
}

/// A link being drawn: aiming (the first point follows the pointer, fixed by the right button's
/// release, or by a left click when started with L), then extending (the far end follows the
/// pointer, a left click finishes).
#[derive(Clone, Debug, PartialEq)]
struct LinkState {
    src: u64,
    hover: bool,
    pressed: bool,
    start: Option<LinkStart>,
    sticky: Sticky,
}

impl LinkState {
    fn new(src: u64, hover: bool) -> Self {
        Self { src, hover, pressed: false, start: None, sticky: NO_STICKY }
    }

    /// No button is held: left clicks drive it.
    fn waiting(&self) -> bool {
        self.hover || self.start.is_some()
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
enum State {
    #[default]
    Idle,
    Pending { button: Button, at: Sample, mods: Mods },
    Draw { press: Pt, was_slide: bool, sticky: Sticky },
    Move { grab: u64, offset_sec: f64, sticky: Sticky },
    Resize { note: u64, edge: Edge, sticky: Sticky },
    Chain { last: u64, sticky: Sticky },
    Link(LinkState),
    /// A drawn slide whose second note waits for its end: a left click places it.
    SlideEnd { creation: Creation, pressed: bool, sticky: Sticky },
    Marquee { from: Sample, add: bool },
    Paste { pressed: bool },
    Grace(GraceEntry),
}

#[derive(Default)]
pub struct NoteEditor {
    pub selection: Selection,
    pub clipboard: Option<Clipboard>,
    state: State,
    down: [bool; 2],
    swallow: bool,
    ignore_seek: bool,
    cycle: Option<(u64, f64)>,
    last: Option<Sample>,
    pub ghost: Option<GhostShape>,
    pub feedback: Option<DrawFeedback>,
    /// Marquee corners in screen space.
    pub marquee: Option<(Pos2, Pos2)>,
}

fn with_ctx<R>(env: &Env, alt: bool, f: impl FnOnce(&EditCtx) -> R) -> R {
    let project: &Project = env.project;
    let tempo = TempoMap::new(&project.measures);
    f(&EditCtx { project, tempo: &tempo, grid: env.grid, px_per_sec: env.px_per_sec, threshold_px: env.threshold_px, onsets: env.onsets, alt })
}

fn pt(at: &Sample) -> Pt {
    Pt { sec: at.sec, midi: at.refined }
}

/// The note under the pointer as a link target: its body, edges or free link points.
fn target_of(hit: Option<(u64, HitZone)>) -> Option<u64> {
    hit.filter(|(_, z)| matches!(z, HitZone::Body | HitZone::LeftEdge | HitZone::RightEdge | HitZone::LinkLeft | HitZone::LinkRight)).map(|h| h.0)
}

fn on_note(hit: Option<(u64, HitZone)>) -> Option<(u64, HitZone)> {
    hit.filter(|(_, z)| matches!(z, HitZone::Body | HitZone::LeftEdge | HitZone::RightEdge))
}

impl NoteEditor {
    pub fn is_resizing(&self) -> bool {
        matches!(self.state, State::Resize { .. })
    }

    /// The grace entry is on and consumes the keys.
    pub fn in_entry(&self) -> bool {
        matches!(self.state, State::Grace(_))
    }

    pub fn is_busy(&self) -> bool {
        self.state != State::Idle
    }

    /// Whether a mode that wants the next click (paste preview, link drawing) is on.
    pub fn in_mode(&self) -> bool {
        match &self.state {
            State::Paste { .. } | State::SlideEnd { .. } => true,
            State::Link(l) => l.waiting(),
            _ => false,
        }
    }

    fn reset(&mut self) {
        self.state = State::Idle;
        self.ghost = None;
        self.feedback = None;
        self.marquee = None;
    }

    fn cancel(&mut self) {
        self.reset();
        self.swallow = self.down.iter().any(|d| *d);
    }

    fn commit(&mut self, env: &mut Env, creation: &Creation, label: &str, select: bool) -> Vec<Effect> {
        match commit_creation(env.project, creation, label) {
            Ok((edit, ids)) => {
                if select && !ids.is_empty() {
                    self.selection.set(ids.clone());
                }
                vec![Effect::Edit { edit: Box::new(edit), merge: false, affected: ids }]
            }
            Err(e) => vec![Effect::Message(e.to_string())],
        }
    }

    pub fn handle(&mut self, env: &mut Env, input: Input) -> Vec<Effect> {
        match input {
            Input::Press { button, at, mods } => self.press(button, at, mods),
            Input::Move { at, mods } => self.moved(env, at, mods),
            Input::Release { button, at, mods } => self.release(env, button, at, mods),
            Input::Spectro(event, mods) => self.spectro(env, event, mods),
            Input::Escape => {
                if self.is_busy() {
                    self.cancel();
                } else {
                    self.selection.clear();
                }
                Vec::new()
            }
            Input::Action(action) => self.action(env, action),
            Input::Entry(key) => self.entry_key(env, key),
            Input::Copy => {
                let tempo = TempoMap::new(&env.project.measures);
                self.clipboard = Clipboard::copy(env.project, &tempo, &self.selection.ids);
                Vec::new()
            }
            Input::Paste => {
                if self.clipboard.is_some() && !self.is_busy() {
                    self.state = State::Paste { pressed: false };
                    if let Some(at) = self.last {
                        self.update_paste(env, &at, false);
                    }
                }
                Vec::new()
            }
        }
    }

    fn press(&mut self, button: Button, at: Sample, mods: Mods) -> Vec<Effect> {
        let idx = usize::from(button == Button::Right);
        self.down[idx] = true;
        self.last = Some(at);
        self.ignore_seek = false;
        if self.swallow {
            return Vec::new();
        }
        match (&mut self.state, button) {
            (State::Idle, _) => {
                if button == Button::Right || at.hit.is_some() {
                    self.state = State::Pending { button, at, mods };
                }
            }
            (State::Paste { .. } | State::SlideEnd { .. }, Button::Right) => self.cancel(),
            (State::Paste { pressed } | State::SlideEnd { pressed, .. }, Button::Left) => *pressed = true,
            (State::Link(l), Button::Left) if l.waiting() => l.pressed = true,
            _ => self.cancel(),
        }
        Vec::new()
    }

    fn moved(&mut self, env: &mut Env, at: Sample, mods: Mods) -> Vec<Effect> {
        self.last = Some(at);
        if self.swallow {
            return Vec::new();
        }
        if let State::Pending { button, at: first, mods: first_mods } = self.state {
            if first.pos.distance(at.pos) < DRAG_PX {
                return Vec::new();
            }
            self.start_drag(env, button, first, first_mods);
        }
        self.update(env, &at, mods);
        Vec::new()
    }

    fn start_drag(&mut self, env: &Env, button: Button, first: Sample, mods: Mods) {
        let project: &Project = env.project;
        let hit = first.hit;
        self.state = match (button, hit) {
            (Button::Left, Some((id, zone @ (HitZone::LeftEdge | HitZone::RightEdge)))) => State::Resize { note: id, edge: if zone == HitZone::LeftEdge { Edge::Start } else { Edge::End }, sticky: NO_STICKY },
            (Button::Left, Some((id, HitZone::Body))) if mods.shift && project.outgoing(id).is_none() => State::Chain { last: id, sticky: NO_STICKY },
            (Button::Left, Some((id, HitZone::Body))) => {
                let tempo = TempoMap::new(&project.measures);
                let start = project.note(id).map_or(first.sec, |n| tempo.pos_to_sec(n.start));
                if !self.selection.contains(id) {
                    self.selection.set(vec![id]);
                }
                State::Move { grab: id, offset_sec: first.sec - start, sticky: NO_STICKY }
            }
            (Button::Right, _) if mods.ctrl => State::Marquee { from: first, add: true },
            (Button::Right, None) => State::Marquee { from: first, add: false },
            (Button::Right, Some((id, HitZone::Body | HitZone::LeftEdge | HitZone::RightEdge | HitZone::LinkLeft | HitZone::LinkRight))) => State::Link(LinkState::new(id, false)),
            _ => State::Idle,
        };
    }

    fn update(&mut self, env: &Env, at: &Sample, mods: Mods) {
        let state = self.state.clone();
        match state {
            State::Move { grab, offset_sec, sticky } => {
                let planned = with_ctx(env, mods.alt, |ctx| plan_move(grab, &self.selection.ids, Pt { sec: at.sec, midi: at.midi }, offset_sec, mods.shift, sticky, ctx).map(|(c, s)| (ghost_of_changes(ctx.project, ctx.tempo, &preview_changes(ctx.project, &c)), s)));
                if let Some((ghost, next)) = planned {
                    self.ghost = Some(ghost);
                    self.state = State::Move { grab, offset_sec, sticky: next };
                }
            }
            State::Resize { note, edge, sticky } => {
                let planned = with_ctx(env, mods.alt, |ctx| plan_resize(note, edge, at.sec, sticky, ctx).map(|(c, s)| (ghost_of_changes(ctx.project, ctx.tempo, &preview_changes(ctx.project, &[c])), s)));
                if let Some((ghost, next)) = planned {
                    self.ghost = Some(ghost);
                    self.state = State::Resize { note, edge, sticky: next };
                }
            }
            State::Chain { last, sticky } => {
                let plan = with_ctx(env, mods.alt, |ctx| plan_chain(last, pt(at), sticky, ctx).map(|p| (ghost_of(&p.creation, ctx.project, ctx.tempo), feedback(&p.creation, ctx.project, ctx.tempo), p.snapped)));
                let next = plan.as_ref().map(|p| p.2);
                self.show_plan(plan);
                if let Some(s) = next {
                    self.state = State::Chain { last, sticky: s };
                }
            }
            State::Link(l) => {
                let plan = with_ctx(env, mods.alt, |ctx| {
                    let extend = |start: &LinkStart, sec, sticky| {
                        let p = plan_extend(start, sec, sticky, ctx);
                        (ghost_of(&p.creation, ctx.project, ctx.tempo), feedback(&p.creation, ctx.project, ctx.tempo), p.snapped)
                    };
                    match &l.start {
                        Some(start) => Some(extend(start, Some(at.sec), l.sticky)),
                        None => match plan_aim(l.src, pt(at), target_of(at.hit), l.sticky, ctx) {
                            LinkAim::Hidden => None,
                            LinkAim::Join(c) => Some((ghost_of(&c, ctx.project, ctx.tempo), feedback(&c, ctx.project, ctx.tempo), NO_STICKY)),
                            LinkAim::Start { start, snapped } => {
                                let (ghost, fb, _) = extend(&start, None, NO_STICKY);
                                Some((ghost, fb, snapped))
                            }
                        },
                    }
                });
                let next = plan.as_ref().map(|p| p.2);
                self.show_plan(plan);
                if let (Some(s), State::Link(l)) = (next, &mut self.state) {
                    l.sticky = s;
                }
            }
            State::Draw { press, was_slide, sticky } => {
                let planned = with_ctx(env, mods.alt, |ctx| plan_draw(press, pt(at), was_slide, sticky, ctx).map(|p| (ghost_of(&p.creation, ctx.project, ctx.tempo), feedback(&p.creation, ctx.project, ctx.tempo), p.snapped, p.slide)));
                match planned {
                    Some((ghost, fb, snapped, slide)) => {
                        self.ghost = Some(ghost);
                        self.feedback = Some(fb);
                        self.state = State::Draw { press, was_slide: slide, sticky: snapped };
                    }
                    None => {
                        self.ghost = None;
                        self.feedback = None;
                        self.state = State::Draw { press, was_slide: false, sticky };
                    }
                }
            }
            State::SlideEnd { creation, sticky, .. } => {
                let (c, next) = with_ctx(env, mods.alt, |ctx| plan_slide_end(&creation, at.sec, sticky, ctx));
                let shown = with_ctx(env, mods.alt, |ctx| (ghost_of(&c, ctx.project, ctx.tempo), feedback(&c, ctx.project, ctx.tempo), next));
                self.show_plan(Some(shown));
                if let State::SlideEnd { sticky, .. } = &mut self.state {
                    *sticky = next;
                }
            }
            State::Marquee { from, .. } => self.marquee = Some((from.pos, at.pos)),
            State::Paste { .. } => self.update_paste(env, at, mods.alt),
            State::Idle | State::Pending { .. } | State::Grace(_) => {}
        }
    }

    fn show_plan(&mut self, plan: Option<(GhostShape, DrawFeedback, Sticky)>) {
        (self.ghost, self.feedback) = match plan {
            Some((ghost, fb, _)) => (Some(ghost), Some(fb)),
            None => (None, None),
        };
    }

    fn paste_anchor(env: &Env, sec: f64, alt: bool) -> fretscribe_core::TimePos {
        with_ctx(env, alt, |ctx| snap_edge(ctx.tempo.sec_to_pos(sec), Edge::Start, ctx.tempo, &[], &ctx.opts(NO_STICKY)))
    }

    fn update_paste(&mut self, env: &Env, at: &Sample, alt: bool) {
        let anchor = Self::paste_anchor(env, at.sec, alt);
        let tempo = TempoMap::new(&env.project.measures);
        self.ghost = self.clipboard.as_ref().map(|c| c.ghost(&tempo, anchor));
    }

    fn release(&mut self, env: &mut Env, button: Button, at: Sample, mods: Mods) -> Vec<Effect> {
        self.down[usize::from(button == Button::Right)] = false;
        self.last = Some(at);
        if self.swallow {
            self.swallow = self.down.iter().any(|d| *d);
            // The click that cancelled a gesture must not move the playhead.
            self.ignore_seek |= button == Button::Left;
            return Vec::new();
        }
        let state = std::mem::take(&mut self.state);
        let effects = match (state, button) {
            (State::Pending { button: b, at: first, mods: m }, _) if b == button => self.click(env, button, first, m),
            (State::Pending { .. }, _) => Vec::new(),
            (State::Move { grab, offset_sec, sticky }, Button::Left) => {
                let changes = with_ctx(env, mods.alt, |ctx| plan_move(grab, &self.selection.ids, Pt { sec: at.sec, midi: at.midi }, offset_sec, mods.shift, sticky, ctx).map(|p| p.0));
                self.finish_changes(env, changes.unwrap_or_default(), "Move notes")
            }
            (State::Resize { note, edge, sticky }, Button::Left) => {
                let change = with_ctx(env, mods.alt, |ctx| plan_resize(note, edge, at.sec, sticky, ctx).map(|p| p.0));
                change.map_or_else(Vec::new, |c| self.finish_resize(env, &c, edge))
            }
            (State::Chain { last, sticky }, Button::Left) => {
                let plan = with_ctx(env, mods.alt, |ctx| plan_chain(last, pt(&at), sticky, ctx));
                plan.map_or_else(Vec::new, |p| self.commit(env, &p.creation, "Extend chain", true))
            }
            (State::Grace(entry), _) => {
                self.state = State::Grace(entry);
                Vec::new()
            }
            (State::Link(l), _) => self.release_link(env, l, button, &at, mods),
            (State::Marquee { from, add }, Button::Right) => {
                let hits = with_ctx(env, false, |ctx| {
                    let times = fretscribe_core::NoteTimes::build(&ctx.project.notes, ctx.tempo);
                    marquee_hits(&ctx.project.notes, &times, (from.sec, at.sec), (from.midi, at.midi))
                });
                self.selection.marquee(&hits, add);
                Vec::new()
            }
            (State::Draw { press, was_slide, sticky }, Button::Left) => self.finish_draw(env, press, was_slide, sticky, pt(&at), mods),
            (State::SlideEnd { creation, pressed: true, sticky }, Button::Left) => {
                self.ignore_seek = true;
                let (c, _) = with_ctx(env, mods.alt, |ctx| plan_slide_end(&creation, at.sec, sticky, ctx));
                self.commit(env, &c, "Draw slide", true)
            }
            (State::Paste { pressed: true }, Button::Left) => {
                self.ignore_seek = true;
                let anchor = Self::paste_anchor(env, at.sec, mods.alt);
                let tempo = TempoMap::new(&env.project.measures);
                match self.clipboard.clone().map(|c| c.commit(env.project, &tempo, anchor, "Paste notes")) {
                    Some(Ok((edit, ids))) => {
                        self.selection.set(ids.clone());
                        vec![Effect::Edit { edit: Box::new(edit), merge: false, affected: ids }]
                    }
                    Some(Err(e)) => vec![Effect::Message(e.to_string())],
                    None => Vec::new(),
                }
            }
            (other @ (State::Paste { .. } | State::SlideEnd { .. }), _) => {
                self.state = other;
                return Vec::new();
            }
            _ => Vec::new(),
        };
        if !self.in_mode() {
            self.reset_preview_only();
        }
        effects
    }

    fn reset_preview_only(&mut self) {
        self.ghost = None;
        self.feedback = None;
        self.marquee = None;
    }

    fn finish_changes(&mut self, env: &mut Env, changes: Vec<NoteChange>, label: &str) -> Vec<Effect> {
        if changes.is_empty() || is_noop(env.project, &changes) {
            return Vec::new();
        }
        let affected = changes.iter().map(|c| c.id).collect();
        match apply_changes(env.project, &changes, label) {
            Ok(edit) => vec![Effect::Edit { edit: Box::new(edit), merge: false, affected }],
            Err(e) => vec![Effect::Message(e.to_string())],
        }
    }

    fn finish_resize(&mut self, env: &mut Env, change: &NoteChange, edge: Edge) -> Vec<Effect> {
        if is_noop(env.project, std::slice::from_ref(change)) {
            return Vec::new();
        }
        match apply_resize(env.project, change, edge, "Resize note") {
            Ok(edit) => vec![Effect::Edit { edit: Box::new(edit), merge: false, affected: vec![change.id] }],
            Err(e) => vec![Effect::Message(e.to_string())],
        }
    }

    /// End of a left drag from empty space: a note is created at once, a slide waits for the click
    /// that sets the end of its second note.
    fn finish_draw(&mut self, env: &mut Env, press: Pt, was_slide: bool, sticky: Sticky, cur: Pt, mods: Mods) -> Vec<Effect> {
        let Some(plan) = with_ctx(env, mods.alt, |ctx| plan_draw(press, cur, was_slide, sticky, ctx)) else { return Vec::new() };
        if !plan.slide {
            return self.commit(env, &plan.creation, "Draw note", true);
        }
        self.state = State::SlideEnd { creation: plan.creation, pressed: false, sticky: (None, None) };
        let at = Sample { pos: Pos2::ZERO, sec: cur.sec, midi: cur.midi, refined: cur.midi, hit: None };
        self.update(env, &at, mods);
        Vec::new()
    }

    /// A button went up while linking: the right one (or a left click when waiting) fixes the first
    /// point, or finishes the link.
    fn release_link(&mut self, env: &mut Env, mut l: LinkState, button: Button, at: &Sample, mods: Mods) -> Vec<Effect> {
        let places = if l.waiting() { button == Button::Left && l.pressed } else { button == Button::Right };
        if !places {
            l.pressed = false;
            self.state = State::Link(l);
            return Vec::new();
        }
        self.ignore_seek |= l.waiting();
        if let Some(start) = &l.start {
            let plan = with_ctx(env, mods.alt, |ctx| plan_extend(start, Some(at.sec), l.sticky, ctx));
            return self.commit(env, &plan.creation, "Draw linked note", true);
        }
        match with_ctx(env, mods.alt, |ctx| plan_aim(l.src, pt(at), target_of(at.hit), l.sticky, ctx)) {
            LinkAim::Hidden => Vec::new(),
            LinkAim::Join(c) => self.commit(env, &c, "Link notes", false),
            LinkAim::Start { start, .. } => {
                self.state = State::Link(LinkState { start: Some(start), pressed: false, sticky: NO_STICKY, ..l });
                self.update(env, at, mods);
                Vec::new()
            }
        }
    }

    fn click(&mut self, env: &mut Env, button: Button, first: Sample, mods: Mods) -> Vec<Effect> {
        let tempo = TempoMap::new(&env.project.measures);
        let order = time_order(&env.project.notes, &tempo);
        match (button, first.hit) {
            (Button::Left, Some((id, _))) if on_note(first.hit).is_some() => {
                self.selection.click(id, mods.ctrl, mods.shift, &order);
                Vec::new()
            }
            (Button::Right, None) => {
                self.selection.clear();
                Vec::new()
            }
            (Button::Right, Some((id, HitZone::Line | HitZone::Arrow))) => self.remove_link(env, id, false),
            (Button::Right, Some((id, HitZone::Arc))) => self.remove_link(env, id, true),
            (Button::Right, Some((id, _))) if on_note(first.hit).is_some() => {
                let mut one = Selection::default();
                one.set(vec![id]);
                let merge = self.cycle.is_some_and(|(note, at)| note == id && env.now - at <= CYCLE_WINDOW_SEC);
                match apply(env.project, &one, NoteAction::CycleLinkType, env.grid) {
                    Ok(done) => {
                        let Some(edit) = done.edit else { return Vec::new() };
                        self.cycle = Some((id, env.now));
                        let mut out = vec![Effect::Edit { edit: Box::new(edit), merge, affected: done.affected }];
                        out.extend(done.message.map(Effect::Message));
                        out
                    }
                    Err(e) => vec![Effect::Message(e.to_string())],
                }
            }
            _ => Vec::new(),
        }
    }

    fn remove_link(&mut self, env: &mut Env, id: u64, by_end: bool) -> Vec<Effect> {
        let from = if by_end { env.project.incoming(id).map(|(from, _)| from) } else { Some(id) };
        let Some(from) = from else { return Vec::new() };
        match env.project.edit_notes("Remove link", |p| {
            p.unlink(from);
            Ok(())
        }) {
            Ok(edit) => vec![Effect::Edit { edit: Box::new(edit), merge: false, affected: vec![from] }],
            Err(e) => vec![Effect::Message(e.to_string())],
        }
    }

    fn spectro(&mut self, env: &mut Env, event: SpectroEvent, mods: Mods) -> Vec<Effect> {
        match event {
            SpectroEvent::SeekRequested(sec) => {
                if std::mem::take(&mut self.ignore_seek) || self.in_mode() {
                    return Vec::new();
                }
                vec![Effect::Seek(sec)]
            }
            SpectroEvent::DrawBegan { exact_midi_float, time_sec, .. } => {
                if self.state == State::Idle && !self.swallow {
                    self.state = State::Draw { press: Pt { sec: time_sec, midi: exact_midi_float }, was_slide: false, sticky: NO_STICKY };
                }
                Vec::new()
            }
            SpectroEvent::DrawMoved { exact_midi_float, time_sec, .. } => {
                if matches!(self.state, State::Draw { .. }) {
                    let at = Sample { pos: Pos2::ZERO, sec: time_sec, midi: exact_midi_float, refined: exact_midi_float, hit: None };
                    self.update(env, &at, mods);
                }
                Vec::new()
            }
            SpectroEvent::DrawEnded { exact_midi_float, time_sec, .. } => {
                let State::Draw { press, was_slide, sticky } = self.state.clone() else { return Vec::new() };
                self.reset();
                self.finish_draw(env, press, was_slide, sticky, Pt { sec: time_sec, midi: exact_midi_float }, mods)
            }
            SpectroEvent::DrawCancelled => {
                self.cancel();
                Vec::new()
            }
        }
    }

    fn show_entry(&mut self, env: &Env, entry: &GraceEntry) {
        (self.ghost, self.feedback) = match entry.preview(env.project) {
            Some((ghost, feedback)) => (Some(ghost), Some(feedback)),
            None => (None, Some(DrawFeedback { warning: Some("no room for a grace note".into()), ..DrawFeedback::default() })),
        };
    }

    fn entry_key(&mut self, env: &mut Env, key: EntryKey) -> Vec<Effect> {
        let State::Grace(mut entry) = std::mem::take(&mut self.state) else { return Vec::new() };
        if entry.key(env.project, key) == EntryStep::Continue {
            self.show_entry(env, &entry);
            self.state = State::Grace(entry);
            return Vec::new();
        }
        match entry.commit(env.project) {
            Ok((edit, affected)) => {
                self.reset();
                self.selection.set(affected[..1].to_vec());
                vec![Effect::Edit { edit: Box::new(edit), merge: false, affected }]
            }
            Err(message) => {
                self.state = State::Grace(entry);
                vec![Effect::Message(message)]
            }
        }
    }

    fn action(&mut self, env: &mut Env, action: NoteAction) -> Vec<Effect> {
        if action == NoteAction::LinkHover && self.selection.ids.is_empty() {
            if let Some(src) = env.hovered.filter(|_| !self.is_busy()) {
                self.state = State::Link(LinkState::new(src, true));
            }
            return Vec::new();
        }
        let merge = action == NoteAction::CycleLinkType && self.cycle.is_some_and(|(_, at)| env.now - at <= CYCLE_WINDOW_SEC);
        match apply(env.project, &self.selection, action, env.grid) {
            Ok(done) => {
                if let Some(selection) = done.selection {
                    self.selection = selection;
                }
                let mut out = Vec::new();
                if action == NoteAction::CycleLinkType && done.edit.is_some() {
                    self.cycle = Some((self.selection.anchor.unwrap_or(0), env.now));
                }
                if let Some(edit) = done.edit {
                    out.push(Effect::Edit { edit: Box::new(edit), merge, affected: done.affected });
                }
                out.extend(done.message.map(Effect::Message));
                out.extend(done.open_exact.map(Effect::OpenExact));
                if let Some(entry) = done.grace_entry.filter(|_| !self.is_busy()).and_then(|main| GraceEntry::new(env.project, main)) {
                    self.show_entry(env, &entry);
                    self.state = State::Grace(entry);
                }
                out
            }
            Err(ActionError::EmptySelection) => Vec::new(),
            Err(e) => vec![Effect::Message(e.to_string())],
        }
    }
}
