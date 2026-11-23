use super::*;

fn grey() -> WaveformPalette {
    let colors: Vec<[u8; 4]> = (0..=255u8).map(|v| [v, v, v, 255]).collect();
    WaveformPalette::from_rgba(&colors)
}

fn wave() -> Rect {
    Rect::from_min_max(pos2(0.0, 0.0), pos2(100.0, 40.0))
}

#[test]
fn palette_lookup_clamps_and_survives_nan() {
    let p = grey();
    assert_eq!(p.color(-1.0), Color32::from_rgb(0, 0, 0));
    assert_eq!(p.color(7.0), Color32::from_rgb(255, 255, 255));
    assert_eq!(p.color(f32::NAN), Color32::from_rgb(0, 0, 0));
    assert_eq!(p.color(0.5), Color32::from_rgb(128, 128, 128));
    assert_eq!(WaveformPalette::from_rgba(&[]), WaveformPalette::default());
    assert_eq!(WaveformPalette::from_rgba(&[[1, 2, 3, 255]]).color(0.7), Color32::from_rgb(1, 2, 3));
}

#[test]
fn lighten_moves_towards_white() {
    assert_eq!(lighten(Color32::BLACK, 0.05), Color32::from_rgb(13, 13, 13));
    assert_eq!(lighten(Color32::from_rgb(10, 200, 30), 0.05), Color32::from_rgb(22, 203, 41));
    assert_eq!(lighten(Color32::from_rgb(9, 8, 7), 0.0), Color32::from_rgb(9, 8, 7));
    assert_eq!(lighten(Color32::from_rgb(9, 8, 7), 5.0), Color32::WHITE);
    assert_eq!(lighten(Color32::BLACK, f32::NAN), Color32::BLACK);
}

#[test]
fn levels_use_the_largest_peak_and_the_gamma() {
    let l = levels(&[(-0.5, 0.2, 0.1), (0.0, 0.0, 0.0), (f32::NAN, 2.0, 0.0)]);
    assert!((l[0] - 0.5f32.powf(1.0 / WAVEFORM_GAMMA)).abs() < 1e-6);
    assert_eq!(l[1], 0.0);
    assert_eq!(l[2], 1.0);
}

#[test]
fn smoothing_averages_loud_columns_only() {
    assert_eq!(smooth_levels(&[1.0, 0.0, 0.5], 0), [1.0, 0.0, 0.5]);
    assert_eq!(smooth_levels(&[1.0, 0.0, 0.5], 1), [1.0, 0.0, 0.5]);
    let s = smooth_levels(&[1.0, 0.0, 0.5], 4);
    assert_eq!(s[1], 0.0, "silent columns stay silent");
    assert!((s[0] - 0.75).abs() < 1e-6 && (s[2] - 0.75).abs() < 1e-6, "{s:?}");
    let s = smooth_levels(&[0.4; 9], 6);
    assert!(s.iter().all(|v| (v - 0.4).abs() < 1e-6));
    assert!(smooth_levels(&[], 20).is_empty());
}

#[test]
fn fade_runs_from_the_centre_to_the_tip() {
    assert_eq!(fade(0.0), 1.0);
    assert!((fade(1.0) - EDGE_SCALE).abs() < 1e-6);
    assert_eq!(fade(5.0), fade(1.0));
}

#[test]
fn silent_columns_draw_nothing_but_the_centre_line() {
    let m = build_mesh(wave(), 1.0, &[(0.0, 0.0, 0.0); 100], &grey());
    assert_eq!(m.vertices.len(), 4);
    assert_eq!(m.indices.len(), 6);
}

#[test]
fn a_loud_column_makes_envelope_and_body_quads() {
    let m = build_mesh(wave(), 1.0, &[(-0.5, 0.5, 0.25)], &grey());
    assert_eq!(m.vertices.len(), 4 + 4 * 4, "centre line, 2 envelope quads, 2 body quads");
    assert_eq!(m.indices.len() % 3, 0);
    assert!(m.indices.iter().all(|&i| (i as usize) < m.vertices.len()));
}

#[test]
fn columns_are_one_physical_pixel_wide_and_stay_in_the_rect() {
    let peaks = vec![(-1.0, 1.0, 0.5); 200];
    let m = build_mesh(wave(), 2.0, &peaks, &grey());
    let bounds = m.calc_bounds();
    assert!(bounds.left() >= 0.0 && bounds.right() <= 100.0 + 1e-3, "{bounds:?}");
    assert!(bounds.top() >= 0.0 && bounds.bottom() <= 40.0, "{bounds:?}");
    assert!(m.vertices[4..8].windows(2).all(|w| (w[1].pos.x - w[0].pos.x).abs() <= 0.5 + 1e-6));
}

#[test]
fn body_is_brighter_than_the_envelope_and_the_tips_are_dimmer() {
    let m = build_mesh(wave(), 1.0, &[(-1.0, 1.0, 0.4)], &grey());
    let luma = |c: Color32| u32::from(c.r());
    // quads in order: upper envelope (tip, centre), lower envelope, upper body, lower body
    let (env_tip, env_centre) = (luma(m.vertices[4].color), luma(m.vertices[6].color));
    let (body_tip, body_centre) = (luma(m.vertices[12].color), luma(m.vertices[14].color));
    assert!(env_tip < env_centre, "{env_tip} {env_centre}");
    assert!(body_centre > env_centre, "{body_centre} {env_centre}");
    assert!(body_tip >= env_tip);
}

#[test]
fn non_finite_peaks_are_treated_as_silence() {
    let m = build_mesh(wave(), 1.0, &[(f32::NAN, f32::INFINITY, f32::NAN); 3], &grey());
    assert!(m.vertices.iter().all(|v| v.pos.x.is_finite() && v.pos.y.is_finite()));
}
