use eframe::egui::{self, Context, Event, LayerId, Pos2, RawInput, Rect};
use fretscribe_core::ProjectFile;

use super::*;
use crate::luts::LutSet;
use crate::settings::Settings;
use crate::store::AudioStore;

struct Harness {
    ctx: Context,
    session: Session,
    width: f32,
    time: f64,
    shapes: Vec<egui::epaint::ClippedShape>,
}

impl Harness {
    fn new(width: f32, ppp: f32) -> Self {
        let ctx = Context::default();
        theme::install_fonts(&ctx);
        kit::install(&ctx);
        theme::apply(&ctx);
        ctx.style_mut(|s| s.interaction.tooltip_delay = f32::INFINITY);
        ctx.set_pixels_per_point(ppp);
        let file = ProjectFile::new(fretscribe_core::testkit::project(2.0));
        let session = Session::new(file, None, AudioStore::default(), &Settings::default(), LutSet::scan(None));
        let mut h = Self { ctx, session, width, time: 0.0, shapes: Vec::new() };
        h.settle();
        h
    }

    fn frame(&mut self, events: Vec<Event>) {
        self.time += 0.05;
        let raw = RawInput { screen_rect: Some(Rect::from_min_size(Pos2::ZERO, egui::vec2(self.width, 800.0))), time: Some(self.time), events, ..Default::default() };
        let session = &mut self.session;
        self.shapes = self.ctx.run(raw, |ctx| {
            crate::toolbar::show(ctx, session);
        })
        .shapes;
    }

    fn settle(&mut self) {
        for _ in 0..4 {
            self.frame(Vec::new());
        }
    }

    fn click(&mut self, pos: Pos2) {
        let button = |pressed| Event::PointerButton { pos, button: PointerButton::Primary, pressed, modifiers: Default::default() };
        self.frame(vec![Event::PointerMoved(pos)]);
        self.frame(vec![button(true)]);
        self.frame(vec![button(false)]);
        self.settle();
    }

    /// The clickable widgets of the layers `wanted` accepts.
    fn widgets(&self, wanted: impl Fn(LayerId) -> bool) -> Vec<Rect> {
        self.ctx.viewport(|v| v.prev_pass.widgets.layers().filter(|(l, _)| wanted(**l)).flat_map(|(_, w)| w).filter(|w| w.sense.interactive()).map(|w| w.rect).collect())
    }

    /// The painted group panels.
    fn panels(&self) -> Vec<Rect> {
        self.shapes.iter().filter_map(|s| match &s.shape {
            egui::Shape::Rect(r) if r.fill == theme::PANEL_BG && r.rect.height() > 40.0 => Some(r.rect),
            _ => None,
        })
        .collect()
    }
}

fn assert_apart(rects: &[Rect], screen: Rect, what: &str) {
    for (i, a) in rects.iter().enumerate() {
        assert!(screen.expand(0.5).contains_rect(*a), "{what} {a:?} leaves {screen:?}");
        for b in &rects[i + 1..] {
            let overlap = a.intersect(*b);
            assert!(overlap.width() <= 0.5 || overlap.height() <= 0.5, "{what} {a:?} and {b:?} overlap");
        }
    }
}

#[test]
fn every_group_lies_inside_the_window_and_none_intersect() {
    for (width, ppp) in [(1920.0, 1.0), (1536.0, 1.25), (1280.0, 1.5), (960.0, 2.0), (900.0, 1.0)] {
        let mut h = Harness::new(width, ppp);
        for expanded in [None, Some(Group::Tuning), Some(Group::Listening)] {
            if let Some(g) = expanded {
                h.session.toolbar.expand(g);
                h.settle();
            }
            let screen = h.ctx.screen_rect();
            let panels = h.panels();
            assert_eq!(panels.len(), Group::ALL.len(), "{width}: {panels:?}");
            assert_apart(&panels, screen, &format!("{width} panel"));
            // The tuning field's preset button lies inside its text box on purpose.
            let widgets: Vec<Rect> = h.widgets(|l| l.order == egui::Order::Middle || l.order == egui::Order::Background).into_iter().filter(|r| r.height() > 15.0 && r.height() < 60.0 && r.height() != 28.0 && r.width() < 300.0).collect();
            assert!(widgets.len() > 20);
            assert_apart(&widgets, screen, &format!("{width} widget"));
        }
    }
}

#[test]
fn picking_from_a_menu_inside_a_dropdown_leaves_the_dropdown_open() {
    let mut h = Harness::new(900.0, 1.0);
    set_open(&h.ctx, Some(Group::Spectrogram));
    h.settle();
    let in_dropdown = |h: &Harness| h.widgets(|l| l.id == open_id());
    // The colour map field is the dropdown's last control.
    let field = in_dropdown(&h).into_iter().filter(|r| r.height() == kit::BUTTON).max_by(|a, b| a.left().total_cmp(&b.left())).unwrap();
    h.click(field.center());
    assert!(h.ctx.memory(|m| m.any_popup_open()), "the colour map menu opens");
    let menu = Id::new(("kit_popup", "lut"));
    let mut rows = h.widgets(|l| l.id == menu);
    rows.retain(|r| r.height() == 28.0);
    rows.sort_by(|a, b| a.top().total_cmp(&b.top()));
    let before = h.session.ui.lut_name.clone();
    let other = h.session.luts.names().iter().position(|n| *n != before).unwrap();
    h.click(rows[other].center());
    assert_ne!(h.session.ui.lut_name, before);
    assert_eq!(open_group(&h.ctx), Some(Group::Spectrogram));

    // Escape closes the child menu first, then the dropdown; so does a click outside.
    h.click(field.center());
    let escape = Event::Key { key: Key::Escape, physical_key: None, pressed: true, repeat: false, modifiers: Modifiers::NONE };
    h.frame(vec![escape.clone()]);
    h.settle();
    assert!(!h.ctx.memory(|m| m.any_popup_open()));
    assert_eq!(open_group(&h.ctx), Some(Group::Spectrogram));
    h.frame(vec![escape]);
    assert_eq!(open_group(&h.ctx), None);
    set_open(&h.ctx, Some(Group::Spectrogram));
    h.settle();
    h.click(Pos2::new(450.0, 700.0));
    assert_eq!(open_group(&h.ctx), None);
}
