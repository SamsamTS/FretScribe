//! Staff notation of a hand-made score: every rhythm, rest, link and grace case.
//! Click a note to select it (blue), click empty space to move the playhead, right click logs a
//! context event. `staff_demo --screenshot out.png` saves the first frames and quits.

use eframe::egui;
use fretscribe_core::{GraceKind, GraceNote, Legato, LegatoLink, Measure, Note, TempoMap, Technique, TimeSignature};
use fretscribe_notation::staff::{layout, KeySignature, StaffOptions};
use fretscribe_ui::staff_view::{install_fonts, StaffStyle, StaffView, StaffViewEvent};
use fretscribe_ui::TimelineView;

const BEAT: f64 = 0.5;

/// (midi, start beat, end beat); ids follow the order.
const SCORE: &[(u8, f64, f64)] = &[
    (52, 0.0, 1.0), (55, 1.0, 1.5), (57, 1.5, 2.0),
    (60, 2.0, 2.333), (62, 2.333, 2.667), (64, 2.667, 3.0),
    (64, 3.0, 3.25), (62, 3.25, 3.5), (60, 3.5, 3.75), (59, 3.75, 4.0),
    (48, 4.0, 5.0), (52, 4.0, 5.0), (55, 4.0, 5.0), (57, 5.0, 6.5), (59, 6.5, 7.0), (52, 7.0, 9.0),
    (45, 10.0, 11.0), (50, 11.0, 12.0), // slide
    (52, 12.0, 12.5), (54, 12.5, 13.0), // bend
    (52, 13.0, 13.5), (55, 13.5, 14.0), // hammer-on
    (57, 14.0, 14.5), (55, 14.5, 15.0), // pull-off
    (60, 16.0, 17.0), (62, 15.9, 16.0), (64, 16.0, 16.1), // main, acciaccatura, appoggiatura
    (61, 17.0, 17.5), (61, 17.5, 18.0), (60, 18.0, 19.0),
    (40, 24.0, 25.0),
];

fn notes(measures: &[Measure]) -> Vec<Note> {
    let map = TempoMap::new(measures);
    let mut notes: Vec<Note> = SCORE.iter().enumerate().map(|(i, &(midi, s, e))| Note::new(i as u64 + 1, midi, map.sec_to_pos(s * BEAT), map.sec_to_pos(e * BEAT))).collect();
    // Index = id - 1.
    for (from, to, technique) in [(17, 18, Technique::Slide), (19, 20, Technique::Bend)] {
        notes[from - 1].link_to = Some(to);
        notes[from - 1].technique = Some(technique);
    }
    notes[21].legato = Some(LegatoLink { kind: Legato::HammerOn, from: 21 });
    notes[23].legato = Some(LegatoLink { kind: Legato::PullOff, from: 23 });
    notes[25].grace_of = Some(GraceNote { main: 25, kind: GraceKind::Acciaccatura });
    notes[26].grace_of = Some(GraceNote { main: 25, kind: GraceKind::Appoggiatura });
    notes
}

struct Demo {
    measures: Vec<Measure>,
    notes: Vec<Note>,
    view: TimelineView,
    style: StaffStyle,
    opts: StaffOptions,
    selected: Vec<u64>,
    playhead: f64,
    log: String,
    shot: Option<std::path::PathBuf>,
    frames: u32,
}

impl eframe::App for Demo {
    fn update(&mut self, ctx: &egui::Context, _: &mut eframe::Frame) {
        egui::TopBottomPanel::top("controls").show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.add(egui::Slider::new(&mut self.opts.key.0, -7..=7).text("key"));
                ui.checkbox(&mut self.opts.guitar_octave, "guitar 8vb");
                ui.add(egui::Slider::new(&mut self.view.pixels_per_sec, 20.0..=300.0).text("px/s"));
                ui.add(egui::Slider::new(&mut self.view.scroll_sec, 0.0..=14.0).text("scroll"));
                ui.label(&self.log);
            });
        });
        egui::CentralPanel::default().show(ctx, |ui| {
            let staff = layout(&self.notes, &self.measures, &StaffOptions { key: KeySignature(self.opts.key.0), ..self.opts.clone() });
            let out = StaffView { layout: &staff, selected: &self.selected, playhead: Some(self.playhead), style: &self.style, timeline_offset: 90.0 }.show(ui, &self.view);
            for e in out.events {
                self.log = format!("{e:?}");
                match e {
                    StaffViewEvent::NoteClicked { id, .. } => self.selected = vec![id],
                    StaffViewEvent::SeekRequested(sec) => self.playhead = sec,
                    StaffViewEvent::NoteContext(_) => {}
                }
            }
        });
        self.frames += 1;
        if let Some(path) = self.shot.clone() {
            match self.frames {
                4 => ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot(egui::UserData::default())),
                _ => ctx.request_repaint(),
            }
            let image = ctx.input(|i| i.events.iter().find_map(|e| if let egui::Event::Screenshot { image, .. } = e { Some(image.clone()) } else { None }));
            if let Some(img) = image {
                let rgba: Vec<u8> = img.pixels.iter().flat_map(|c| c.to_array()).collect();
                let _ = image::save_buffer(path, &rgba, img.width() as u32, img.height() as u32, image::ColorType::Rgba8);
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            }
        }
    }
}

fn main() -> eframe::Result {
    let mut next = 1;
    let measures = fretscribe_core::generate_measures(0.0, 14.0, 120.0, TimeSignature::COMMON, &mut next);
    let shot = std::env::args().skip_while(|a| a != "--screenshot").nth(1).map(Into::into);
    let demo = Demo {
        notes: notes(&measures),
        measures,
        view: TimelineView::new(0.0, 100.0),
        style: StaffStyle::default(),
        opts: StaffOptions::default(),
        selected: vec![11],
        playhead: 1.0,
        log: String::new(),
        shot,
        frames: 0,
    };
    eframe::run_native(
        "FretScribe staff demo",
        eframe::NativeOptions { viewport: egui::ViewportBuilder::default().with_inner_size([1500.0, 260.0]), ..Default::default() },
        Box::new(|cc| {
            cc.egui_ctx.set_visuals(egui::Visuals::dark());
            install_fonts(&cc.egui_ctx);
            Ok(Box::new(demo))
        }),
    )
}
