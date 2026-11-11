//! Tablature of a hand-made melody: right click a number for the placements, drag it to another
//! string, Up/Down (adjacent string) and Tab (next placement) while hovering, Alt+click cycles,
//! click the left column for the tuning popup. Ctrl+Z / Ctrl+Y undo and redo.

use eframe::egui;
use fretscribe_core::{next_note_id, LinkType, Note, NoteTimes, Project, TempoMap, TimePos, TimeSignature, Track, TrackRole, UndoStack};
use fretscribe_notation::staff::{layout, StaffOptions};
use fretscribe_notation::tab::{assign_tabs, TabOptions};
use fretscribe_ui::tab_view::{apply, apply_tuning, TabEdit, TabStyle, TabView, TabViewEvent, TabViewState};
use fretscribe_ui::TimelineView;

struct Demo {
    project: Project,
    undo: UndoStack,
    state: TabViewState,
    view: TimelineView,
    style: TabStyle,
    selected: Vec<u64>,
    opts: TabOptions,
    log: String,
}

fn project() -> Project {
    let mut p = Project::new(Track::new(1, "Guitar", TrackRole::Guitar), 12.0, 120.0, TimeSignature::COMMON);
    let melody = [(52, 0.0, 0.5), (55, 0.5, 1.0), (57, 1.0, 1.5), (59, 1.5, 2.0), (64, 2.0, 3.0), (66, 3.0, 4.0), (64, 4.0, 5.0), (62, 5.0, 5.5), (60, 5.5, 6.0), (45, 6.0, 7.0), (50, 7.0, 8.0)];
    for (midi, s, e) in melody {
        let id = next_note_id(&p);
        p.notes.push(Note::new(id, midi, TimePos::new(1, s), TimePos::new(1, e)));
    }
    let _ = p.link_notes(5, 6, LinkType::Bend);
    let _ = p.link_notes(6, 7, LinkType::Bend);
    let _ = p.link_notes(10, 11, LinkType::Slide);
    let _ = p.link_notes(2, 3, LinkType::Legato);
    let times = NoteTimes::build(&p.notes, &TempoMap::new(&p.measures));
    assign_tabs(&mut p.notes, &times, &p.tuning, p.capo, &TabOptions::default());
    p
}

impl eframe::App for Demo {
    fn update(&mut self, ctx: &egui::Context, _: &mut eframe::Frame) {
        if ctx.input_mut(|i| i.consume_key(egui::Modifiers::COMMAND, egui::Key::Z)) {
            self.undo.undo(&mut self.project);
        }
        if ctx.input_mut(|i| i.consume_key(egui::Modifiers::COMMAND, egui::Key::Y)) {
            self.undo.redo(&mut self.project);
        }
        egui::TopBottomPanel::top("controls").show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.add(egui::Slider::new(&mut self.view.pixels_per_sec, 20.0..=300.0).text("px/s"));
                ui.label(&self.log);
            });
        });
        egui::CentralPanel::default().show(ctx, |ui| {
            let times = NoteTimes::build(&self.project.notes, &TempoMap::new(&self.project.measures));
            let staff = layout(&self.project.notes, &self.project.measures, &StaffOptions::default());
            let out = TabView { project: &self.project, times: &times, staff: Some(&staff), selected: &self.selected, playhead: None, style: &self.style, opts: &self.opts, timeline_offset: 90.0 }.show(ui, &self.view, &mut self.state);
            let now = ctx.input(|i| i.time);
            for e in out.events {
                match e {
                    TabViewEvent::NoteClicked { id, ctrl, .. } => {
                        if ctrl { self.selected.push(id) } else { self.selected = vec![id] }
                    }
                    TabViewEvent::Edit(edit) => self.edit(&edit, now),
                    TabViewEvent::TuningChanged { tuning, capo } => {
                        let edit = apply_tuning(&mut self.project, tuning, capo, &self.opts);
                        self.undo.push(edit);
                    }
                    TabViewEvent::SeekRequested(_) => {}
                }
            }
        });
    }
}

impl Demo {
    fn edit(&mut self, edit: &TabEdit, now: f64) {
        match apply(&mut self.project, edit, &self.opts, self.state.propagate_backwards) {
            Ok(done) => {
                self.state.flash(&done.changed, now);
                self.undo.push(done.edit);
                self.log.clear();
            }
            Err(e) => self.log = e.to_string(),
        }
    }
}

fn main() -> eframe::Result {
    let demo = Demo { project: project(), undo: UndoStack::new(), state: TabViewState::default(), view: TimelineView::new(0.0, 110.0), style: TabStyle::default(), selected: vec![], opts: TabOptions::default(), log: String::new() };
    eframe::run_native(
        "FretScribe tab demo",
        eframe::NativeOptions { viewport: egui::ViewportBuilder::default().with_inner_size([1300.0, 320.0]), ..Default::default() },
        Box::new(|cc| {
            cc.egui_ctx.set_visuals(egui::Visuals::dark());
            Ok(Box::new(demo))
        }),
    )
}
