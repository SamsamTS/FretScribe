//! Left-button gestures that start on empty space: a click seeks, a drag draws.

use egui::{Key, PointerButton, Pos2, Rect, Response, Ui};

use super::{SpectroEvent, SpectroGeometry};

pub const DRAG_THRESHOLD_PX: f32 = 4.0;

#[derive(Clone, Copy)]
struct Press {
    pos: Pos2,
    began: bool,
    /// Cancelled: nothing more comes out of this press, not even its release.
    swallowed: bool,
}

fn sample(geo: &SpectroGeometry, pos: Pos2) -> (u8, f32, f64) {
    let exact = geo.y_to_midi(pos.y);
    (exact.round().clamp(0.0, 127.0) as u8, exact, geo.x_to_sec(pos.x).max(0.0))
}

/// `empty_at` tells whether a press position is free of notes and links.
pub fn events(ui: &Ui, response: &Response, geo: &SpectroGeometry, ruler: Rect, empty_at: impl Fn(Pos2) -> bool) -> Vec<SpectroEvent> {
    let key = response.id.with("press");
    let mut state: Option<Press> = ui.data(|d| d.get_temp::<Option<Press>>(key)).flatten();
    let (pressed, released, down, other_pressed, esc, latest) = ui.input(|i| {
        let p = &i.pointer;
        (
            p.button_pressed(PointerButton::Primary),
            p.button_released(PointerButton::Primary),
            p.button_down(PointerButton::Primary),
            [PointerButton::Secondary, PointerButton::Middle].iter().any(|b| p.button_pressed(*b)),
            i.key_pressed(Key::Escape),
            p.latest_pos(),
        )
    });
    let mut out = Vec::new();

    if let (true, None, Some(pos)) = (pressed, state, latest) {
        if response.contains_pointer() && !ruler.contains(pos) && empty_at(pos) {
            state = Some(Press { pos, began: false, swallowed: false });
        }
    }
    if let Some(mut press) = state {
        let cur = latest.unwrap_or(press.pos);
        if (other_pressed || esc) && !press.swallowed {
            press.swallowed = true;
            if press.began {
                out.push(SpectroEvent::DrawCancelled);
            }
        }
        if !press.swallowed {
            if !press.began && down && cur.distance(press.pos) >= DRAG_THRESHOLD_PX {
                press.began = true;
                let (midi, exact_midi_float, time_sec) = sample(geo, press.pos);
                out.push(SpectroEvent::DrawBegan { midi, exact_midi_float, time_sec });
            }
            if press.began {
                let (midi, exact_midi_float, time_sec) = sample(geo, cur);
                if released {
                    out.push(SpectroEvent::DrawEnded { midi, exact_midi_float, time_sec });
                } else if down {
                    out.push(SpectroEvent::DrawMoved { midi, exact_midi_float, time_sec });
                }
            } else if released {
                out.push(SpectroEvent::SeekRequested(sample(geo, press.pos).2));
            }
        }
        state = (down && !released).then_some(press);
    }
    ui.data_mut(|d| d.insert_temp(key, state));
    out
}
