use egui::{CentralPanel, Context, RawInput};
use fretscribe_core::{generate_measures, Note, TempoMap, TimeSignature};
use fretscribe_notation::staff::{layout, StaffOptions};

use super::*;

#[test]
fn ledger_lines_stop_at_the_note() {
    assert_eq!(ledger_steps(-4), vec![-4, -2]);
    assert_eq!(ledger_steps(-3), vec![-2]);
    assert_eq!(ledger_steps(10), vec![10]);
    assert_eq!(ledger_steps(11), vec![10]);
    assert!(ledger_steps(0).is_empty() && ledger_steps(8).is_empty());
}

#[test]
fn dead_notes_use_x_noteheads_by_duration() {
    let d = Some(Technique::Dead);
    assert_eq!([Base::Whole, Base::Half, Base::Quarter].map(|b| head_char(b, d)), ["\u{E0A7}", "\u{E0A8}", "\u{E0A9}"]);
    assert_eq!(head_char(Base::Quarter, None), "\u{E0A4}");
    assert_eq!([Base::Whole, Base::Half, Base::Eighth].map(|b| head_char(b, Some(Technique::PinchHarmonic))), ["\u{E0D8}", "\u{E0D9}", "\u{E0DB}"]);
}

#[test]
fn glyph_codes() {
    assert_eq!(flag_char(1, true), '\u{E240}');
    assert_eq!(flag_char(2, false), '\u{E243}');
    assert_eq!(digits(12), "\u{E081}\u{E082}");
    assert_eq!(rest_char(Base::Quarter), '\u{E4E5}');
}

#[test]
fn draws_a_layout_without_panicking() {
    let mut next = 1;
    let measures = generate_measures(0.0, 8.0, 120.0, TimeSignature::COMMON, &mut next);
    let map = TempoMap::new(&measures);
    let notes: Vec<Note> = (0..8).map(|i| Note::new(i + 1, 50 + i as u8, map.sec_to_pos(i as f64 * 0.25), map.sec_to_pos(i as f64 * 0.25 + 0.25))).collect();
    let staff = layout(&notes, &measures, &StaffOptions::default());
    let ctx = Context::default();
    install_fonts(&ctx);
    let style = StaffStyle::default();
    for _ in 0..2 {
        let input = RawInput { screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(800.0, 300.0))), ..Default::default() };
        let _ = ctx.run(input, |ctx| {
            CentralPanel::default().show(ctx, |ui| {
                let out = StaffView { layout: &staff, selected: &[3], playhead: Some(0.5), style: &style, grid: &NoteGrid::default(), timeline_offset: 60.0 }.show(ui, &TimelineView::new(0.0, 200.0));
                assert!(out.events.is_empty());
            });
        });
    }
}
