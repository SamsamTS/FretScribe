use super::*;
use fretscribe_core::{Measure, TimeSignature};

fn tempo() -> TempoMap {
    let ms: Vec<Measure> = (0u32..8).map(|i| Measure::new(u64::from(i), f64::from(i) * 2.0, 120.0, TimeSignature::new(4, 4))).collect();
    TempoMap::new(&ms)
}

#[test]
fn notes_stay_inside_their_cell_and_clear_of_lines() {
    let t = tempo();
    for pps in [40.0, 80.0, 200.0, 600.0] {
        let view = TimelineView::new(0.0, pps);
        let g = NoteGrid::new(&t, &view, 1000.0);
        let mut last = f32::MIN;
        for k in 0..600 {
            let sec = f64::from(k) * 0.01;
            let x = g.x(&view, sec);
            let i = g.lines.partition_point(|l| l.0 <= sec + EPS) - 1;
            let (a, b) = (view.sec_to_x(g.lines[i].0), g.lines.get(i + 1).map(|l| view.sec_to_x(l.0)));
            assert!(x - GLYPH_W / 2.0 >= a + MARGIN - 0.01, "left pps={pps} sec={sec}");
            assert!(b.is_none_or(|b| x + GLYPH_W / 2.0 <= b - MARGIN + 0.01), "right pps={pps} sec={sec}");
            assert!(x >= last - 0.01, "monotone");
            last = x;
        }
    }
}

#[test]
fn lines_halve_as_zoom_drops_and_measures_stay() {
    let t = tempo();
    let counts: Vec<usize> = [400.0, 150.0, 60.0, 20.0].iter().map(|p| NoteGrid::new(&t, &TimelineView::new(0.0, *p), 1000.0).lines.iter().filter(|l| l.0 < 2.0).count()).collect();
    assert!(counts.windows(2).all(|w| w[0] >= w[1]) && counts[0] > counts[3], "{counts:?}");
    let g = NoteGrid::new(&t, &TimelineView::new(0.0, 20.0), 1000.0);
    assert!(g.lines.iter().all(|l| l.1 == BeatKind::Measure));
}

#[test]
fn meter_does_not_matter_for_the_guarantee() {
    let mut ms: Vec<Measure> = (0u32..4).map(|i| Measure::new(u64::from(i), f64::from(i) * 1.5, 160.0, TimeSignature::new(4, 4))).collect();
    ms[1].time_sig = TimeSignature::new(6, 8);
    let g = NoteGrid::new(&TempoMap::new(&ms), &TimelineView::new(0.0, 100.0), 600.0);
    assert!(g.lines.windows(2).all(|w| w[0].0 < w[1].0));
}
