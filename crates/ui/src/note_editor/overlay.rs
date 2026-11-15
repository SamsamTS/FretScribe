//! Live feedback painted over the source lane: marquee and the labels of a note being drawn.

use egui::{pos2, vec2, Align2, Color32, FontId, Painter, Pos2, Rect, Stroke, StrokeKind};

use fretscribe_core::LinkType;

use super::NoteEditor;
use crate::spectro::SpectroGeometry;

const WARNING: Color32 = Color32::from_rgb(255, 190, 70);

fn label(painter: &Painter, at: Pos2, text: &str, color: Color32) {
    let galley = painter.layout_no_wrap(text.to_owned(), FontId::monospace(11.0), color);
    let rect = Rect::from_min_size(at, galley.size()).expand2(vec2(4.0, 2.0));
    painter.rect_filled(rect, 3.0, Color32::from_black_alpha(210));
    painter.galley(at, galley, color);
}

/// `toast` is the short message of the last command (for instance the new link type), shown at
/// the pointer.
pub fn paint(painter: &Painter, geo: &SpectroGeometry, editor: &NoteEditor, pointer: Option<Pos2>, toast: Option<&str>) {
    if let Some((a, b)) = editor.marquee {
        let rect = Rect::from_two_pos(a, b);
        painter.rect_filled(rect, 0.0, Color32::from_white_alpha(18));
        painter.rect_stroke(rect, 0.0, Stroke::new(1.0_f32, Color32::from_white_alpha(170)), StrokeKind::Inside);
    }
    if let Some(feedback) = &editor.feedback {
        for (sec, midi, text) in &feedback.labels {
            let at = pos2(geo.sec_to_x(*sec), geo.midi_to_y(f32::from(*midi)) - 20.0);
            label(painter, at, text, Color32::WHITE);
        }
        if let Some(p) = pointer {
            if let Some(interval) = &feedback.interval {
                let kind = feedback.link.map_or("", |k| match k {
                    LinkType::Slide => "Slide ",
                    LinkType::Bend => "Bend ",
                    LinkType::Legato => "Legato ",
                    LinkType::Tied => "Tie ",
                    LinkType::Dashed => "",
                });
                label(painter, p + vec2(14.0, -26.0), &format!("{kind}{interval}"), Color32::WHITE);
            }
            if let Some(warning) = &feedback.warning {
                label(painter, p + vec2(14.0, 8.0), warning, WARNING);
            }
        }
    }
    if let (Some(text), Some(p)) = (toast, pointer) {
        painter.text(p + vec2(14.0, -34.0), Align2::LEFT_BOTTOM, text, FontId::proportional(12.0), Color32::WHITE);
    }
}
