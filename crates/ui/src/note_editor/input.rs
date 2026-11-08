//! egui to editor: turns the frame's pointer, keys and spectrogram events into `Input`s.

use egui::{Context, Event, Id, Key, PointerButton, Pos2, Rect};
use fretscribe_core::{Note, NoteTimes};

use super::editor::{Button, Input, Mods, NoteEditor, Sample};
use super::NoteAction;
use crate::spectro::{hit_test, HitZone, SpectroEvent, SpectroGeometry};

pub struct SourcePointer<'a> {
    pub geometry: SpectroGeometry,
    pub hovered_note: Option<(u64, HitZone)>,
    pub pitch_ruler: Rect,
    pub notes: &'a [Note],
    pub times: &'a NoteTimes,
}

pub struct Collected {
    pub inputs: Vec<Input>,
    /// A note was double-clicked: open the exact-edit popup for it.
    pub open_exact: Option<u64>,
}

fn mods_of(ctx: &Context) -> Mods {
    ctx.input(|i| Mods { shift: i.modifiers.shift, ctrl: i.modifiers.command, alt: i.modifiers.alt })
}

/// `refine(time_sec, midi)` returns the pitch centred on the spectrogram peak (project time in,
/// the caller applies the track offset). Keys are skipped while a text field has the focus.
pub fn collect(ctx: &Context, src: &SourcePointer, spectro_events: &[SpectroEvent], editor: &NoteEditor, refine: &dyn Fn(f64, f32) -> f32) -> Collected {
    let mods = mods_of(ctx);
    let geo = &src.geometry;
    let hover_id = src.hovered_note.map(|h| h.0);
    let sample = |pos: Pos2| {
        let (midi, sec) = (geo.y_to_midi(pos.y), geo.x_to_sec(pos.x).max(0.0));
        Sample { pos, sec, midi, refined: refine(sec, midi), hit: hit_test(geo, src.notes, src.times, pos, hover_id) }
    };
    let key = Id::new("note-editor-pressed");
    let mut pressed: [bool; 2] = ctx.data(|d| d.get_temp(key)).unwrap_or_default();
    let pos = ctx.input(|i| i.pointer.latest_pos());
    let inside = |p: Pos2| geo.rect.contains(p) && !src.pitch_ruler.contains(p);
    let buttons = [(PointerButton::Primary, Button::Left), (PointerButton::Secondary, Button::Right)];
    let mut inputs = Vec::new();

    for (i, (pb, button)) in buttons.iter().enumerate() {
        if let Some(p) = pos.filter(|p| inside(*p) && ctx.input(|inp| inp.pointer.button_pressed(*pb))) {
            pressed[i] = true;
            inputs.push(Input::Press { button: *button, at: sample(p), mods });
        }
    }
    if let Some(p) = pos.filter(|_| pressed.iter().any(|d| *d) || editor.in_mode()) {
        inputs.push(Input::Move { at: sample(p), mods });
    }
    for (i, (pb, button)) in buttons.iter().enumerate() {
        if pressed[i] && ctx.input(|inp| inp.pointer.button_released(*pb)) {
            pressed[i] = false;
            if let Some(p) = pos {
                inputs.push(Input::Release { button: *button, at: sample(p), mods });
            }
        }
    }
    ctx.data_mut(|d| d.insert_temp(key, pressed));

    let mut cancelled = false;
    for event in spectro_events {
        let event = match event {
            SpectroEvent::DrawBegan { midi, exact_midi_float, time_sec } => SpectroEvent::DrawBegan { midi: *midi, exact_midi_float: refine(*time_sec, *exact_midi_float), time_sec: *time_sec },
            SpectroEvent::DrawMoved { midi, exact_midi_float, time_sec } => SpectroEvent::DrawMoved { midi: *midi, exact_midi_float: refine(*time_sec, *exact_midi_float), time_sec: *time_sec },
            SpectroEvent::DrawEnded { midi, exact_midi_float, time_sec } => SpectroEvent::DrawEnded { midi: *midi, exact_midi_float: refine(*time_sec, *exact_midi_float), time_sec: *time_sec },
            other => other.clone(),
        };
        cancelled |= event == SpectroEvent::DrawCancelled;
        inputs.push(Input::Spectro(event, mods));
    }

    if !ctx.wants_keyboard_input() {
        ctx.input(|i| {
            if i.key_pressed(Key::Escape) && !cancelled {
                inputs.push(Input::Escape);
            }
            if i.key_pressed(Key::Delete) || i.key_pressed(Key::Backspace) {
                inputs.push(Input::Action(NoteAction::Delete { bridge: i.modifiers.shift }));
            }
            if i.modifiers.command && i.key_pressed(Key::D) {
                inputs.push(Input::Action(NoteAction::Duplicate));
            }
            if i.modifiers.command && i.key_pressed(Key::A) {
                inputs.push(Input::Action(NoteAction::SelectAll));
            }
            if !i.modifiers.command && !i.modifiers.alt {
                for (key, by) in [(Key::ArrowUp, 1), (Key::ArrowDown, -1)] {
                    if i.key_pressed(key) {
                        inputs.push(Input::Action(NoteAction::Transpose(by)));
                    }
                }
                if i.key_pressed(Key::L) {
                    inputs.push(Input::Action(NoteAction::LinkHover));
                }
            }
            for event in &i.events {
                match event {
                    Event::Copy => inputs.push(Input::Copy),
                    Event::Paste(_) => inputs.push(Input::Paste),
                    _ => {}
                }
            }
        });
    }
    let double = ctx.input(|i| i.pointer.button_double_clicked(PointerButton::Primary));
    let open_exact = double.then(|| pos.and_then(|p| sample(p).hit)).flatten().and_then(|(id, zone)| matches!(zone, HitZone::Body | HitZone::LeftEdge | HitZone::RightEdge).then_some(id));
    Collected { inputs, open_exact }
}
