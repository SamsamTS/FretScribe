//! Tuning editor popup: preset, one pitch per string, add / remove strings, capo.

use egui::{DragValue, Window};
use fretscribe_core::{midi_to_name, Tuning};
use fretscribe_notation::tab::{presets, string_label};

use super::TabViewEvent;
use crate::note_editor::parse_pitch;

pub const MAX_CAPO: u8 = 12;

#[derive(Clone, Debug, PartialEq)]
pub struct TuningDraft {
    pub tuning: Tuning,
    pub capo: u8,
}

pub(super) fn show(ctx: &egui::Context, draft: &mut Option<TuningDraft>, events: &mut Vec<TabViewEvent>) {
    let Some(d) = draft.as_mut() else { return };
    let (mut apply, mut cancel) = (false, false);
    Window::new("Tuning").collapsible(false).resizable(false).show(ctx, |ui| {
        egui::ComboBox::from_label("Preset").selected_text(d.tuning.name.clone()).show_ui(ui, |ui| {
            for t in presets() {
                if ui.selectable_label(t.name == d.tuning.name, &t.name).clicked() {
                    d.tuning = t;
                }
            }
        });
        let mut remove = None;
        let mut edited = false;
        for i in (0..d.tuning.open_strings.len()).rev() {
            ui.horizontal(|ui| {
                ui.label(string_label(&d.tuning, i as u8).unwrap_or_default());
                let mut midi = d.tuning.open_strings[i];
                let value = DragValue::new(&mut midi).range(0..=127).custom_formatter(|v, _| midi_to_name(v as u8)).custom_parser(|s| parse_pitch(s).map(f64::from));
                if ui.add(value).changed() {
                    d.tuning.open_strings[i] = midi;
                    edited = true;
                }
                if d.tuning.open_strings.len() > 1 && ui.small_button("✕").on_hover_text("Remove this string").clicked() {
                    remove = Some(i);
                }
            });
        }
        if let Some(i) = remove {
            d.tuning.open_strings.remove(i);
            edited = true;
        }
        if ui.button("+ Add string").clicked() {
            let top = d.tuning.open_strings.last().copied().unwrap_or(60);
            d.tuning.open_strings.push(top.saturating_add(5).min(127));
            edited = true;
        }
        if edited {
            d.tuning.name = "Custom".to_owned();
        }
        ui.horizontal(|ui| {
            ui.label("Capo");
            ui.add(DragValue::new(&mut d.capo).range(0..=MAX_CAPO));
        });
        ui.horizontal(|ui| {
            apply = ui.button("Apply").clicked();
            cancel = ui.button("Cancel").clicked();
        });
    });
    if apply {
        events.push(TabViewEvent::TuningChanged { tuning: d.tuning.clone(), capo: d.capo });
    }
    if apply || cancel {
        *draft = None;
    }
}
