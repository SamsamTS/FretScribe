//! Demo of the measure grid over a plain rectangle standing in for the spectrogram.
//!
//! The wheel handling below is demo-only: in the application the navigation layer owns it.

use eframe::egui::{self, Key, KeyboardShortcut, Modifiers, Sense};
use fretscribe_core::{
    EditError, NotePolicy, Note, Project, SnapGrid, TempoMap, TimePos, TimeSignature, Track,
    TrackRole, UndoStack,
};
use fretscribe_ui::grid::policy_label;
use fretscribe_ui::{GridEvent, GridOverlay, TimelineView};

const AUDIO_LEN_SEC: f64 = 40.0;
const UNDO: KeyboardShortcut = KeyboardShortcut::new(Modifiers::COMMAND, Key::Z);
const REDO: KeyboardShortcut = KeyboardShortcut::new(Modifiers::COMMAND, Key::Y);
const REDO_SHIFT: KeyboardShortcut = KeyboardShortcut::new(Modifiers::COMMAND.plus(Modifiers::SHIFT), Key::Z);

struct GridDemo {
    project: Project,
    undo: UndoStack,
    view: TimelineView,
    snap: SnapGrid,
    policy: NotePolicy,
    status: String,
    pending_delete: Option<GridEvent>,
}

impl GridDemo {
    fn new() -> Self {
        let mut project = Project::new(
            Track::new(1, "Guitar", TrackRole::Guitar),
            AUDIO_LEN_SEC,
            100.0,
            TimeSignature::COMMON,
        );
        let id = project.measures[2].id;
        for (n, midi) in [(1_u64, 52), (2, 55)] {
            let beat = n as f64;
            project.notes.push(Note::new(n, midi, TimePos::new(id, beat), TimePos::new(id, beat + 1.0)));
        }
        Self {
            project,
            undo: UndoStack::new(),
            view: TimelineView::default(),
            snap: SnapGrid::default(),
            policy: NotePolicy::FollowGrid,
            status: "Drag a barline, click a label, right-click a barline. Alt = other note policy.".to_owned(),
            pending_delete: None,
        }
    }

    fn handle(&mut self, event: GridEvent) {
        match event {
            GridEvent::NotePolicyChanged(policy) => self.policy = policy,
            GridEvent::AutoFitRequested { index } => self.status = on_request_autofit(index),
            _ => match event.apply(&mut self.project, false) {
                Some(Ok(edit)) => {
                    self.status = edit.label().to_owned();
                    self.undo.push(edit);
                }
                Some(Err(EditError::NeedsConfirmation { .. })) => self.pending_delete = Some(event),
                Some(Err(error)) => self.status = error.to_string(),
                None => {}
            },
        }
    }

    fn undo_redo(&mut self, ctx: &egui::Context) {
        if ctx.input_mut(|i| i.consume_shortcut(&REDO_SHIFT) || i.consume_shortcut(&REDO)) {
            self.undo.redo(&mut self.project);
        } else if ctx.input_mut(|i| i.consume_shortcut(&UNDO)) {
            self.undo.undo(&mut self.project);
        }
    }

    fn toolbar(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            let undo_label = self.undo.undo_label().map(|l| format!("Undo {l}")).unwrap_or_else(|| "Undo".to_owned());
            if ui.add_enabled(self.undo.can_undo(), egui::Button::new(undo_label)).clicked() {
                self.undo.undo(&mut self.project);
            }
            let redo_label = self.undo.redo_label().map(|l| format!("Redo {l}")).unwrap_or_else(|| "Redo".to_owned());
            if ui.add_enabled(self.undo.can_redo(), egui::Button::new(redo_label)).clicked() {
                self.undo.redo(&mut self.project);
            }
            ui.separator();
            egui::ComboBox::from_label("Snap")
                .selected_text(self.snap.label())
                .show_ui(ui, |ui| {
                    for preset in SnapGrid::presets() {
                        ui.selectable_value(&mut self.snap, preset.grid, preset.name);
                    }
                });
            egui::ComboBox::from_label("Notes")
                .selected_text(policy_label(self.policy))
                .show_ui(ui, |ui| {
                    for policy in [NotePolicy::FollowGrid, NotePolicy::KeepAudioPosition] {
                        ui.selectable_value(&mut self.policy, policy, policy_label(policy));
                    }
                });
            ui.separator();
            ui.label(&self.status);
        });
    }

    fn confirm_delete(&mut self, ctx: &egui::Context) {
        let Some(event) = self.pending_delete else { return };
        let mut decision = None;
        egui::Window::new("Delete measure?")
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .show(ctx, |ui| {
                ui.label("This measure holds notes; they will be deleted with it.");
                ui.horizontal(|ui| {
                    if ui.button("Delete").clicked() {
                        decision = Some(true);
                    }
                    if ui.button("Cancel").clicked() {
                        decision = Some(false);
                    }
                });
            });
        let Some(confirmed) = decision else { return };
        self.pending_delete = None;
        if confirmed {
            if let Some(Ok(edit)) = event.apply(&mut self.project, true) {
                self.status = edit.label().to_owned();
                self.undo.push(edit);
            }
        }
    }
}

/// Stand-in for the beat detection of the analysis crate.
fn on_request_autofit(measure_index: usize) -> String {
    format!("Auto-fit requested from measure {} (analysis not wired yet)", measure_index + 1)
}

impl eframe::App for GridDemo {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.undo_redo(ctx);
        egui::TopBottomPanel::top("toolbar").show(ctx, |ui| self.toolbar(ui));
        egui::CentralPanel::default().show(ctx, |ui| {
            let (rect, response) = ui.allocate_exact_size(ui.available_size(), Sense::hover());
            if response.hovered() {
                let (scroll, zoom, pointer) = ctx.input(|i| (i.smooth_scroll_delta, i.zoom_delta(), i.pointer.hover_pos()));
                if let Some(pointer) = pointer.filter(|_| zoom != 1.0) {
                    self.view.zoom_around(pointer.x - rect.left(), f64::from(zoom));
                }
                self.view.scroll_sec -= f64::from(scroll.x + scroll.y) / self.view.pixels_per_sec;
                self.view.clamp(AUDIO_LEN_SEC);
            }
            ui.painter().rect_filled(rect, 0.0, egui::Color32::from_rgb(24, 38, 58));

            let tempo = TempoMap::new(&self.project.measures);
            let events = GridOverlay::new(&tempo, &self.view, &self.snap)
                .note_policy(self.policy)
                .autofit(true)
                .show(ui, rect);
            for event in events {
                self.handle(event);
            }
        });
        self.confirm_delete(ctx);
    }
}

fn main() -> eframe::Result {
    eframe::run_native(
        "FretScribe grid demo",
        eframe::NativeOptions::default(),
        Box::new(|cc| {
            cc.egui_ctx.set_visuals(egui::Visuals::dark());
            Ok(Box::new(GridDemo::new()))
        }),
    )
}
