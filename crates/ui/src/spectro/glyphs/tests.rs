use egui::{pos2, vec2, Rect};
use fretscribe_core::{generate_measures, Legato, LegatoLink, TempoMap, TimeSignature};

use super::super::hit_test;
use super::*;
use crate::view::{PitchView, TimelineView};

const H: f32 = 20.0;

fn geo() -> SpectroGeometry {
    SpectroGeometry {
        rect: Rect::from_min_size(pos2(100.0, 50.0), vec2(800.0, 24.0 * H)),
        view: TimelineView::new(0.0, 100.0),
        pitch: PitchView { min_midi: 40.0, max_midi: 64.0 },
    }
}

fn metrics() -> NoteMetrics {
    geo().metrics()
}

type Spec = (u64, u8, f64, f64, Option<u64>, Option<Technique>);

fn setup(specs: &[Spec]) -> (Vec<Note>, NoteTimes) {
    let mut next = 1;
    let map = TempoMap::new(&generate_measures(0.0, 20.0, 120.0, TimeSignature::COMMON, &mut next));
    let notes: Vec<Note> = specs
        .iter()
        .map(|&(id, midi, start, end, link, technique)| {
            let mut n = Note::new(id, midi, map.sec_to_pos(start), map.sec_to_pos(end));
            n.link_to = link;
            n.technique = technique;
            n
        })
        .collect();
    let times = NoteTimes::build(&notes, &map);
    (notes, times)
}

fn legato(notes: &mut [Note], to: usize, from: u64) {
    notes[to].legato = Some(LegatoLink { kind: Legato::HammerOn, from });
}

fn glyphs(notes: &[Note], times: &NoteTimes) -> Vec<Glyph> {
    note_glyphs(&geo(), notes, times, &metrics())
}

fn rect_of(g: &[Glyph], id: u64) -> Rect {
    g.iter().find_map(|g| if let Glyph::Note(r, i) = g { (*i == id).then_some(*r) } else { None }).unwrap()
}

fn link_of(g: &[Glyph]) -> (Vec<Pos2>, Option<Pos2>) {
    g.iter().find_map(|g| if let Glyph::Link { path, arrow_tip, .. } = g { Some((path.clone(), *arrow_tip)) } else { None }).unwrap()
}

fn arc_of(g: &[Glyph]) -> Vec<Pos2> {
    g.iter().find_map(|g| if let Glyph::Arc { path, .. } = g { Some(path.clone()) } else { None }).unwrap()
}

fn near(a: Pos2, b: Pos2) {
    assert!(a.distance(b) < 1e-3, "{a:?} != {b:?}");
}

fn mid_of_cubic(p0: Pos2, c1: Pos2, c2: Pos2, p3: Pos2) -> Pos2 {
    pos2((p0.x + 3.0 * c1.x + 3.0 * c2.x + p3.x) / 8.0, (p0.y + 3.0 * c1.y + 3.0 * c2.y + p3.y) / 8.0)
}

#[test]
fn metrics_follow_the_row_height_and_small_notes_lose_their_name() {
    assert_eq!(NoteMetrics::new(5.0).height_px, 5.0);
    let small = NoteMetrics::new(8.0);
    assert!(!label_visible(Rect::from_min_size(pos2(0.0, 0.0), vec2(80.0, 8.0)), 2, &[], &small));
    assert_eq!(metrics().height_px, H);
}

#[test]
fn plain_note_keeps_its_borders_and_is_at_least_3px_wide() {
    let (notes, times) = setup(&[(1, 52, 1.0, 2.0, None, None), (2, 52, 3.0, 3.0, None, None)]);
    let g = glyphs(&notes, &times);
    let (a, b) = (rect_of(&g, 1), rect_of(&g, 2));
    assert!((a.left() - 200.0).abs() < 1e-3 && (a.right() - 300.0).abs() < 1e-3);
    assert!((a.height() - H).abs() < 1e-3 && (a.center().y - geo().midi_to_y(52.0)).abs() < 1e-3);
    assert!((b.width() - MIN_NOTE_PX).abs() < 1e-3);
}

#[test]
fn short_chain_note_is_a_square_to_the_right_and_its_left_edge_never_moves() {
    let (notes, times) = setup(&[(1, 52, 1.0, 1.05, Some(2), None), (2, 52, 3.0, 3.05, None, None)]);
    let g = glyphs(&notes, &times);
    for id in [1, 2] {
        let r = rect_of(&g, id);
        assert!((r.width() - H).abs() < 1e-3, "square when there is room: {r:?}");
    }
    assert!((rect_of(&g, 1).left() - 200.0).abs() < 1e-3 && (rect_of(&g, 2).left() - 400.0).abs() < 1e-3);
}

#[test]
fn square_width_shrinks_with_a_close_next_note_or_a_big_jump_and_the_run_rule_holds() {
    let run = |dest: f64, jump: u8| {
        let (notes, times) = setup(&[(1, 52, 1.0, 1.05, Some(2), None), (2, 52 + jump, dest, dest + 1.0, None, None)]);
        let g = glyphs(&notes, &times);
        let (src, dst) = (rect_of(&g, 1), rect_of(&g, 2));
        (src.width(), dst.left() - src.right(), (dst.center().y - src.center().y).abs())
    };
    let (w, rise, dy) = run(1.2, 1);
    assert!((w - 16.0).abs() < 1e-3 && (rise - 0.2 * dy).abs() < 1e-3, "{w} {rise} {dy}");
    let (w, ..) = run(1.1, 5);
    assert!((w - 5.0).abs() < 1e-3, "the real width wins when the rule is already broken: {w}");
    let (w, ..) = run(1.5, 0);
    assert!((w - H).abs() < 1e-3);

    let (notes, times) = setup(&[(1, 52, 1.0, 1.3, Some(2), None), (2, 53, 1.2, 2.0, None, None)]);
    assert!((rect_of(&glyphs(&notes, &times), 1).width() - 30.0).abs() < 1e-3, "a wide real note is untouched");
}

#[test]
fn last_chain_note_stops_before_the_next_note_in_its_pitch_row() {
    let (notes, times) = setup(&[(1, 50, 0.0, 0.5, Some(2), None), (2, 52, 1.0, 1.02, None, None), (3, 52, 1.1, 2.0, None, None), (4, 53, 1.05, 2.0, None, None)]);
    assert!((rect_of(&glyphs(&notes, &times), 2).width() - 10.0).abs() < 1e-3);
}

#[test]
fn slide_leaves_the_right_edge_and_arrives_at_the_left_border() {
    let (notes, times) = setup(&[(1, 50, 1.0, 2.0, Some(2), Some(Technique::Slide)), (2, 55, 3.0, 4.0, None, None)]);
    let g = glyphs(&notes, &times);
    let (path, tip) = link_of(&g);
    assert_eq!((path.len(), tip), (2, None));
    near(path[0], rect_of(&g, 1).right_center());
    near(path[1], rect_of(&g, 2).left_center());
}

#[test]
fn flipping_slide_and_bend_changes_only_the_path() {
    let build = |t| {
        let (notes, times) = setup(&[(1, 50, 1.0, 2.0, Some(2), Some(t)), (2, 53, 3.0, 4.0, None, None)]);
        glyphs(&notes, &times)
    };
    let (slide, bend) = (build(Technique::Slide), build(Technique::Bend));
    let notes_of = |g: &[Glyph]| g.iter().filter(|g| matches!(g, Glyph::Note(..))).cloned().collect::<Vec<_>>();
    assert_eq!(notes_of(&slide), notes_of(&bend));
    assert_ne!(link_of(&slide).0, link_of(&bend).0);
    let ends = |g: &[Glyph]| g.iter().find_map(|g| if let Glyph::Link { from_note, to_note, .. } = g { Some((*from_note, *to_note)) } else { None });
    assert_eq!(ends(&slide), ends(&bend));
}

#[test]
fn bend_arrives_vertically_with_a_straight_section_and_the_specified_curve() {
    let h = H;
    for (to_midi, up) in [(53u8, true), (47, false)] {
        let (notes, times) = setup(&[(1, 50, 1.0, 2.0, Some(2), Some(Technique::Bend)), (2, to_midi, 3.0, 3.1, None, None)]);
        let g = glyphs(&notes, &times);
        let (path, tip) = link_of(&g);
        let (src, dst) = (rect_of(&g, 1), rect_of(&g, 2));
        let tip = tip.unwrap();
        let tip_y = if up { dst.bottom() } else { dst.top() };
        near(tip, pos2(dst.left() + h.min(dst.width()) / 2.0, tip_y));
        near(*path.last().unwrap(), tip);
        near(path[0], src.right_center());
        let end = path[path.len() - 2];
        near(end, pos2(tip.x, tip.y + if up { 0.2 * h } else { -0.2 * h }));

        let start = path[0];
        let (dx, dy) = (end.x - start.x, end.y - start.y);
        let reach = h.min(0.66 * dy.abs());
        let c2 = end + vec2(0.0, if up { reach } else { -reach });
        near(path[8], mid_of_cubic(start, start + vec2(0.74 * dx, 0.0), c2, end));
        assert!((path[path.len() - 3].x - tip.x).abs() < 0.2 * dx.abs(), "tangent is vertical at the end");
    }
}

#[test]
fn source_overlapping_the_destination_gives_a_vertical_line() {
    let (notes, times) = setup(&[(1, 50, 1.0, 3.0, Some(2), Some(Technique::Slide)), (2, 53, 2.0, 3.0, None, None)]);
    let g = glyphs(&notes, &times);
    let (path, _) = link_of(&g);
    assert!((path[0].x - path[1].x).abs() < 1e-3 && (path[1].x - rect_of(&g, 2).left()).abs() < 1e-3);
}

#[test]
fn arcs_follow_the_control_points_for_hammer_on_and_pull_off() {
    for (a, b, bulges_left) in [(50u8, 55u8, true), (55, 50, false)] {
        let (mut notes, times) = setup(&[(1, a, 1.0, 1.5, None, None), (2, b, 1.5, 2.0, None, None)]);
        legato(&mut notes, 1, 1);
        let g = glyphs(&notes, &times);
        let path = arc_of(&g);
        let pts = link_points(&geo(), &notes, &times, &metrics(), None);
        let dot = |id, side| pts.iter().find(|p| p.note_id == id && p.side == side).unwrap().pos;
        let (from, to) = (dot(1, Side::Right), dot(2, Side::Left));
        let (low, high) = if a < b { (from, to) } else { (to, from) };
        near(path[0], low);
        near(*path.last().unwrap(), high);
        let d = high - low;
        let expected = mid_of_cubic(low, low + vec2(0.0, 0.4 * d.y), high + vec2(-0.6 * d.x, -0.26 * d.y), high);
        near(path[8], expected);
        let chord_mid = low.x + d.x / 2.0;
        assert_eq!(path[8].x < chord_mid, bulges_left);
    }
}

#[test]
fn link_dots_sit_on_edges_for_slides_and_inset_for_legato() {
    let (notes, times) = setup(&[(1, 50, 1.0, 2.0, Some(2), Some(Technique::Slide)), (2, 52, 3.0, 4.0, None, None)]);
    let g = glyphs(&notes, &times);
    let pts = link_points(&geo(), &notes, &times, &metrics(), None);
    assert_eq!(pts.len(), 2);
    near(pts[0].pos, rect_of(&g, 1).right_center());
    near(pts[1].pos, rect_of(&g, 2).left_center());

    let (mut notes, times) = setup(&[(1, 50, 1.0, 2.0, None, None), (2, 52, 2.0, 2.05, None, None)]);
    legato(&mut notes, 1, 1);
    let g = glyphs(&notes, &times);
    let pts = link_points(&geo(), &notes, &times, &metrics(), None);
    near(pts[0].pos, pos2(rect_of(&g, 1).right() - 0.4 * H, rect_of(&g, 1).center().y));
    near(pts[1].pos, pos2(rect_of(&g, 2).left() + 2.5, rect_of(&g, 2).center().y));
}

#[test]
fn bend_ends_with_an_arrowhead_not_a_dot() {
    let (notes, times) = setup(&[(1, 50, 1.0, 2.0, Some(2), Some(Technique::Bend)), (2, 52, 3.0, 4.0, None, None)]);
    let pts = link_points(&geo(), &notes, &times, &metrics(), None);
    assert_eq!(pts.iter().map(|p| (p.note_id, p.side)).collect::<Vec<_>>(), vec![(1, Side::Right)]);
}

#[test]
fn free_link_points_appear_only_on_the_hovered_note_and_only_where_no_link_is_attached() {
    let s = Some(Technique::Slide);
    let (notes, times) = setup(&[(1, 50, 0.5, 1.0, None, None), (2, 50, 2.0, 3.0, Some(3), s), (3, 52, 4.0, 5.0, Some(4), s), (4, 52, 6.0, 7.0, None, None)]);
    let sides = |id| link_points(&geo(), &notes, &times, &metrics(), Some(id)).into_iter().filter(|p| p.free && p.note_id == id).map(|p| p.side).collect::<Vec<_>>();
    assert!(link_points(&geo(), &notes, &times, &metrics(), None).iter().all(|p| !p.free));
    assert_eq!(sides(1), vec![Side::Left, Side::Right]);
    let g = glyphs(&notes, &times);
    let free: Vec<_> = link_points(&geo(), &notes, &times, &metrics(), Some(1)).into_iter().filter(|p| p.free).collect();
    near(free[0].pos, rect_of(&g, 1).left_center() + vec2(0.4 * H, 0.0));
    near(free[1].pos, rect_of(&g, 1).right_center() - vec2(0.4 * H, 0.0));
    assert_eq!(sides(2), vec![Side::Left]);
    assert!(sides(3).is_empty());
    assert_eq!(sides(4), vec![Side::Right]);
}

#[test]
fn name_is_hidden_when_a_dot_touches_its_box_and_shown_otherwise() {
    let m = metrics();
    let r = Rect::from_min_size(pos2(100.0, 100.0), vec2(60.0, H));
    assert!(label_visible(r, 2, &[], &m));
    assert!(label_visible(r, 2, &[r.left_center()], &m));
    assert!(!label_visible(r, 2, &[r.center()], &m));
    let box_edge = name_box(r, 2, &m).right_center();
    assert!(!label_visible(r, 2, &[box_edge + vec2(m.dot_radius() * 0.5, 0.0)], &m));
    assert!(!label_visible(Rect::from_min_size(r.min, vec2(0.8 * H, H)), 2, &[], &m), "too narrow");
}

#[test]
fn corner_radius_shrinks_with_the_width() {
    let m = metrics();
    assert!((corner_radius(&m, 100.0) - 0.1 * H).abs() < 1e-6);
    assert!((corner_radius(&m, 4.0) - 1.0).abs() < 1e-6);
}

#[test]
fn hit_priority_is_dot_then_path_then_edge_then_body() {
    let s = Some(Technique::Slide);
    let (mut notes, times) = setup(&[(1, 50, 1.0, 2.0, Some(2), s), (2, 55, 3.0, 4.0, None, None), (3, 45, 1.0, 2.0, None, None), (4, 48, 2.0, 3.0, None, None)]);
    legato(&mut notes, 3, 3);
    let g = geo();
    let glyph = glyphs(&notes, &times);
    let at = |p| hit_test(&g, &notes, &times, p, None);
    let (r1, r2) = (rect_of(&glyph, 1), rect_of(&glyph, 2));
    assert_eq!(at(r1.right_center() + vec2(3.0, 0.0)), Some((1, HitZone::LinkRight)));
    assert_eq!(at(r2.left_center() - vec2(3.0, 0.0)), Some((2, HitZone::LinkLeft)));
    let (path, _) = link_of(&glyph);
    assert_eq!(at(path[0].lerp(path[1], 0.5)), Some((1, HitZone::Line)));
    assert_eq!(at(r1.left_center() + vec2(2.0, 0.0)), Some((1, HitZone::LeftEdge)));
    assert_eq!(at(r1.center()), Some((1, HitZone::Body)));
    assert_eq!(at(r1.right_center() - vec2(9.0, 0.0)), Some((1, HitZone::Body)), "no resize edge where a link is attached");
    let arc = arc_of(&glyph);
    let on_arc = arc[8] + vec2(0.0, -1.0);
    assert_eq!(at(on_arc), Some((4, HitZone::Arc)));
    assert_eq!(at(pos2(900.0, 500.0)), None);
}

#[test]
fn bend_hits_as_an_arrow_and_free_points_need_the_hover() {
    let (notes, times) = setup(&[(1, 50, 1.0, 2.0, Some(2), Some(Technique::Bend)), (2, 55, 3.0, 4.0, None, None), (3, 45, 6.0, 7.0, None, None)]);
    let g = geo();
    let glyph = glyphs(&notes, &times);
    let (path, _) = link_of(&glyph);
    assert_eq!(hit_test(&g, &notes, &times, path[8], None), Some((1, HitZone::Arrow)));
    let left = rect_of(&glyph, 3).left_center() + vec2(0.4 * H, 0.0);
    assert_eq!(hit_test(&g, &notes, &times, left, None), Some((3, HitZone::Body)));
    assert_eq!(hit_test(&g, &notes, &times, left, Some(3)), Some((3, HitZone::LinkLeft)));
}

#[test]
fn chevron_arms_point_back_from_the_tip() {
    let m = metrics();
    let [a, tip, b] = chevron(pos2(10.0, 10.0), true, &m);
    assert!(a.y > tip.y && b.y > tip.y && (a.distance(tip) - 0.2 * H * 2f32.sqrt()).abs() < 1e-3);
    assert!(chevron(tip, false, &m)[0].y < tip.y);
}
