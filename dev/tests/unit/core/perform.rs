use super::*;
use crate::model::{GraceKind, GraceNote, Legato, LegatoLink, TimePos};
use crate::testkit::{add, project};

const T: PerformTuning = PerformTuning::DEFAULT;
/// Seconds per beat of the test project (120 bpm).
const B: f64 = 0.5;

fn note(p: &mut Project, id: u64) -> &mut Note {
    p.notes.iter_mut().find(|n| n.id == id).unwrap()
}

fn link(p: &mut Project, from: u64, to: u64, technique: Technique) {
    let n = note(p, from);
    (n.link_to, n.technique) = (Some(to), Some(technique));
}

fn close(a: f64, b: f64) {
    assert!((a - b).abs() < 1e-6, "{a} != {b}");
}

/// A chain of notes given as `(midi, start beat, end beat)`, joined by `links`.
fn chain(notes: &[(u8, f64, f64)], links: &[Technique]) -> PlayedNote {
    let mut p = project(8.0);
    let ids: Vec<u64> = notes.iter().map(|&(m, s, e)| add(&mut p, m, s, e)).collect();
    for (pair, &technique) in ids.windows(2).zip(links) {
        link(&mut p, pair[0], pair[1], technique);
    }
    let mut played = performance(&p, false);
    assert_eq!(played.len(), 1);
    played.remove(0)
}

fn at(n: &PlayedNote, beat: f64) -> f32 {
    value_at(&n.pitch_curve, beat * B)
}

#[test]
fn a_slide_is_one_linear_event_gliding_just_before_the_destination() {
    let n = chain(&[(60, 0.0, 1.0), (65, 1.0, 2.0)], &[Technique::Slide]);
    close(n.start_sec, 0.0);
    close(n.end_sec, 2.0 * B);
    assert_eq!(n.midi, 60);
    assert_eq!(n.pitch_curve, vec![(B - T.min_ramp_sec, 0.0), (B, 5.0)]);
    close(f64::from(value_at(&n.pitch_curve, B - T.min_ramp_sec / 2.0)), 2.5);
}

#[test]
fn windows_follow_the_gap_and_stay_inside_the_source() {
    let gap = chain(&[(60, 0.0, 1.0), (62, 1.5, 2.0)], &[Technique::Slide]);
    assert_eq!(gap.pitch_curve, vec![(B, 0.0), (1.5 * B, 2.0)]);
    let long = chain(&[(60, 0.0, 1.0), (62, 4.0, 5.0)], &[Technique::Slide]);
    close(long.pitch_curve[0].0, 4.0 * B - T.max_ramp_sec);
    let tiny = chain(&[(60, 1.0, 1.0625), (62, 1.0625, 2.0)], &[Technique::Slide]);
    close(tiny.pitch_curve[0].0, B);
}

#[test]
fn bends_are_smooth_and_chains_continue_the_curve() {
    let bend = chain(&[(60, 0.0, 1.0), (62, 1.0, 2.0)], &[Technique::Bend]);
    assert!(bend.pitch_curve.len() > 5);
    assert!(bend.pitch_curve.windows(2).all(|w| w[1].1 >= w[0].1 && w[1].0 - w[0].0 <= T.curve_step_sec + 1e-9));
    assert!(bend.pitch_curve[1].1 < 0.2, "eased in");
    close(f64::from(at(&bend, 2.0)), 2.0);

    let release = chain(&[(62, 0.0, 1.0), (60, 1.0, 2.0)], &[Technique::Bend]);
    close(f64::from(at(&release, 0.0)), 0.0);
    close(f64::from(at(&release, 1.0)), -2.0);

    let notes = [(60, 0.0, 1.0), (62, 1.0, 2.0), (60, 2.0, 3.0)];
    let up_down = chain(&notes, &[Technique::Bend, Technique::Bend]);
    close(up_down.end_sec, 3.0 * B);
    close(f64::from(at(&up_down, 1.5)), 2.0);
    close(f64::from(at(&up_down, 3.0)), 0.0);

    let slide_bend = chain(&[(60, 0.0, 1.0), (64, 1.0, 2.0), (66, 2.0, 3.0)], &[Technique::Slide, Technique::Bend]);
    close(f64::from(at(&slide_bend, 1.5)), 4.0);
    close(f64::from(at(&slide_bend, 3.0)), 6.0);

    // Pre-bend: the first note is the bent pitch, released into the second.
    let pre = chain(&[(62, 0.0, 0.0625), (60, 0.0625, 2.0)], &[Technique::Bend]);
    close(f64::from(at(&pre, 2.0)), -2.0);
    close(pre.pitch_curve[0].0, 0.0);
}

#[test]
fn ties_join_without_a_curve_also_inside_a_chain() {
    let tie = chain(&[(60, 0.0, 1.0), (60, 1.0, 2.0)], &[Technique::Tie]);
    close(tie.end_sec, 2.0 * B);
    assert!(tie.pitch_curve.is_empty());
    let n = chain(&[(60, 0.0, 1.0), (60, 1.0, 2.0), (63, 2.0, 3.0)], &[Technique::Tie, Technique::Slide]);
    close(f64::from(at(&n, 1.5)), 0.0);
    close(f64::from(at(&n, 3.0)), 3.0);
}

#[test]
fn legato_is_a_soft_new_event_that_cuts_its_source() {
    let mut p = project(8.0);
    let (a, b, c) = (add(&mut p, 60, 0.0, 1.0), add(&mut p, 62, 1.0, 2.0), add(&mut p, 64, 2.5, 3.0));
    link(&mut p, a, b, Technique::Slide);
    note(&mut p, c).legato = Some(LegatoLink { kind: Legato::HammerOn, from: b });
    let played = performance(&p, false);
    assert_eq!(played.len(), 2);
    close(played[0].end_sec, 2.5 * B);
    assert_eq!((played[1].attack, played[1].velocity), (Attack::Soft, T.soft_velocity));
    assert_eq!((played[0].attack, played[0].velocity), (Attack::Pick, T.velocity));
}

#[test]
fn grace_notes_end_before_their_main_note_which_keeps_its_start() {
    for kind in [GraceKind::Acciaccatura, GraceKind::Appoggiatura] {
        let mut p = project(8.0);
        let main = add(&mut p, 64, 2.0, 3.0);
        let (s, e) = if kind == GraceKind::Acciaccatura { (1.875, 2.0) } else { (2.0, 2.25) };
        let grace = add(&mut p, 62, s, e);
        note(&mut p, grace).grace_of = Some(GraceNote { main, kind });
        note(&mut p, main).legato = Some(LegatoLink { kind: Legato::HammerOn, from: grace });
        let played = performance(&p, false);
        let g = played.iter().find(|n| n.id == grace).unwrap();
        let m = played.iter().find(|n| n.id == main).unwrap();
        close(m.start_sec, 2.0 * B);
        close(g.end_sec, 2.0 * B);
        close(g.end_sec - g.start_sec, (e - s) * B);
        assert_eq!((g.velocity, m.attack), (T.grace_velocity, Attack::Soft));
    }
}

#[test]
fn articulations_set_kind_velocity_and_length() {
    let mut p = project(8.0);
    let kinds = [
        (Technique::Dead, Articulation::Dead),
        (Technique::PalmMute, Articulation::Muted),
        (Technique::Harmonic, Articulation::Harmonic),
        (Technique::PinchHarmonic, Articulation::Harmonic),
    ];
    for (i, (technique, _)) in kinds.iter().enumerate() {
        let id = add(&mut p, 60, i as f64, i as f64 + 1.0);
        note(&mut p, id).technique = Some(*technique);
    }
    let played = performance(&p, false);
    assert_eq!(played.iter().map(|n| n.kind).collect::<Vec<_>>(), kinds.map(|k| k.1));
    close(played[0].end_sec - played[0].start_sec, T.dead_sec);
    assert_eq!(played[0].velocity, T.dead_velocity);
    close(played[1].end_sec - played[1].start_sec, T.palm_mute_max_sec);
    close(played[2].end_sec, 3.0 * B);
}

#[test]
fn vibrato_waits_then_oscillates_over_its_own_note() {
    let mut p = project(8.0);
    let id = add(&mut p, 60, 0.0, 2.0);
    note(&mut p, id).vibrato = true;
    let curve = performance(&p, false).remove(0).pitch_curve;
    close(curve[0].0, T.vibrato_delay_sec);
    let depth = (T.vibrato_cents / 100.0) as f32;
    let peak = curve.iter().map(|c| c.1.abs()).fold(0.0, f32::max);
    assert!(peak <= depth + 1e-6 && peak > depth * 0.9);
    close(f64::from(value_at(&curve, 0.05)), 0.0);
    close(f64::from(value_at(&curve, 1.0)), 0.0);

    let mut p = project(8.0);
    let (a, b) = (add(&mut p, 60, 0.0, 1.0), add(&mut p, 62, 1.0, 4.0));
    link(&mut p, a, b, Technique::Bend);
    note(&mut p, b).vibrato = true;
    let curve = performance(&p, false).remove(0).pitch_curve;
    let around = |from: f64, to: f64| curve.iter().filter(move |c| c.0 > from && c.0 < to).map(|c| c.1);
    assert!(around(0.0, B - 1e-6).all(|v| (0.0..=2.0).contains(&v)), "none on the source");
    let held = around(B + T.vibrato_delay_sec, 4.0 * B);
    let (lo, hi) = held.fold((f32::MAX, f32::MIN), |m, v| (m.0.min(v), m.1.max(v)));
    assert!(lo < 2.0 - depth * 0.9 && hi > 2.0 + depth * 0.9);
}

#[test]
fn let_ring_extends_only_where_ring_ends_says() {
    let mut p = project(8.0);
    let (a, b) = (add(&mut p, 60, 0.0, 1.0), add(&mut p, 64, 4.0, 5.0));
    for id in [a, b] {
        note(&mut p, id).string_fret = Some((1, 3));
    }
    p.set_let_ring(0, true, false).unwrap();
    let played = performance(&p, false);
    close(played[0].end_sec, 4.0 * B);
    close(played[1].end_sec, 5.0 * B);
    note(&mut p, a).technique = Some(Technique::PalmMute);
    assert!(performance(&p, false)[0].end_sec < B);
    // A legato from a palm-muted note does not lengthen it.
    note(&mut p, b).legato = Some(LegatoLink { kind: Legato::HammerOn, from: a });
    assert!(performance(&p, false)[0].end_sec < B);
}

#[test]
fn a_chord_stays_separate_and_an_empty_project_is_silent() {
    let mut p = project(8.0);
    assert!(performance(&p, false).is_empty());
    for midi in [60, 64, 67] {
        add(&mut p, midi, 1.0, 2.0);
    }
    let played = performance(&p, false);
    assert_eq!(played.iter().map(|n| n.midi).collect::<Vec<_>>(), [60, 64, 67]);
    assert!(played.iter().all(|n| n.start_sec == B && n.end_sec == 2.0 * B));
    assert_eq!(p.notes[0].start, TimePos::at(1, 1.0));
}

#[test]
fn auto_vibrato_swells_then_fades_over_a_held_single_note() {
    let mut p = project(8.0);
    add(&mut p, 60, 0.0, 4.0);
    let curve = performance(&p, true).remove(0).pitch_curve;
    let (len, depth) = (4.0 * B, (T.vibrato_cents / 100.0) as f32);
    let widest = |from: f64, to: f64| curve.iter().filter(|c| c.0 >= from * len && c.0 < to * len).map(|c| c.1.abs()).fold(0.0, f32::max);
    assert!(widest(0.28, 0.38) > 0.85 * depth);
    assert!(widest(0.0, 0.08) < 0.3 * depth && widest(0.92, 1.0) < 0.2 * depth);
    assert!(widest(0.55, 0.75) < widest(0.33, 0.5));
    assert!(curve.iter().all(|c| c.1.abs() <= depth));
    assert_eq!((curve[0].1, curve[curve.len() - 1].1), (0.0, 0.0));
    assert!(performance(&p, false)[0].pitch_curve.is_empty());
}

#[test]
fn auto_vibrato_needs_a_long_enough_plain_single_note() {
    let swells = |p: &Project| performance(p, true).iter().map(|n| !n.pitch_curve.is_empty()).collect::<Vec<_>>();
    let mut p = project(8.0);
    add(&mut p, 60, 0.0, 1.0);
    add(&mut p, 60, 1.0, 1.9375);
    add(&mut p, 60, 2.0, 3.0);
    add(&mut p, 64, 2.0, 3.0);
    assert_eq!(swells(&p), [true, false, false, false]);

    let mut p = project(8.0);
    for (i, technique) in [Technique::Dead, Technique::PalmMute].into_iter().enumerate() {
        let id = add(&mut p, 60, i as f64, i as f64 + 1.0);
        note(&mut p, id).technique = Some(technique);
    }
    let main = add(&mut p, 64, 3.0, 4.0);
    let grace = add(&mut p, 62, 2.5, 3.0);
    note(&mut p, grace).grace_of = Some(GraceNote { main, kind: GraceKind::Acciaccatura });
    assert_eq!(swells(&p), [false, false, false, true]);

    // A tie is one held note, too short here without its second half; a slide is not.
    let mut p = project(8.0);
    let (a, b) = (add(&mut p, 60, 0.0, 0.5), add(&mut p, 60, 0.5, 1.0));
    link(&mut p, a, b, Technique::Tie);
    assert_eq!(swells(&p), [true]);
    let (c, d) = (add(&mut p, 62, 2.0, 3.0), add(&mut p, 65, 3.0, 4.0));
    link(&mut p, c, d, Technique::Slide);
    let slide = performance(&p, true).remove(1).pitch_curve;
    assert_eq!(slide, performance(&p, false)[1].pitch_curve);
}

#[test]
fn no_auto_vibrato_while_an_earlier_note_rings_on() {
    let mut p = project(8.0);
    let (a, b) = (add(&mut p, 60, 0.0, 1.0), add(&mut p, 64, 2.0, 3.0));
    // After the barline, where the ringing stops.
    add(&mut p, 67, 4.0, 5.0);
    for (id, string) in [(a, 1), (b, 2)] {
        note(&mut p, id).string_fret = Some((string, 3));
    }
    let swells = |p: &Project| performance(p, true).iter().map(|n| !n.pitch_curve.is_empty()).collect::<Vec<_>>();
    assert_eq!(swells(&p), [true, true, true]);
    p.set_let_ring(0, true, false).unwrap();
    assert_eq!(swells(&p), [true, false, true]);
}
