//! LUT-coloured waveform for the overview strip. It reproduces the colouring of
//! `fretscribe_analysis::waveform::render_rgba` (same constants) on egui meshes; `ui` has no
//! dependency on the analysis crate, so the app hands over the palette as plain colours.

use egui::{pos2, Color32, Mesh, Rect};

pub const PALETTE_SIZE: usize = 256;
/// Keep in sync with `fretscribe_analysis::waveform` (`WAVEFORM_GAMMA`, the colour smoothing and
/// the private level constants).
pub const WAVEFORM_GAMMA: f32 = 1.25;
pub const COLOR_SMOOTHING_PX: usize = 20;
const ENVELOPE_SHARE: f32 = 0.8;
const BODY_LIFT: f32 = 0.3;
const EDGE_SCALE: f32 = 0.7;
const CENTRE_LINE_LEVEL: f32 = 0.2;

const CLASSIC_STOPS: [(f32, [u8; 3]); 4] = [
    (0.0, [0, 0, 0]),
    (0.3, [0, 20, 140]),
    (0.65, [0, 190, 70]),
    (1.0, [255, 235, 0]),
];

/// 256 colours, index 0 the weakest and 255 the strongest, like `analysis::lut::Lut::colors`.
#[derive(Clone, Debug, PartialEq)]
pub struct WaveformPalette {
    colors: Vec<Color32>,
}

impl Default for WaveformPalette {
    /// The built-in "Classic" palette, for when no LUT could be loaded.
    fn default() -> Self {
        let colors = (0..PALETTE_SIZE)
            .map(|i| {
                let t = i as f32 / (PALETTE_SIZE - 1) as f32;
                let [r, g, b] = gradient_at(t);
                Color32::from_rgb(r, g, b)
            })
            .collect();
        Self { colors }
    }
}

fn gradient_at(t: f32) -> [u8; 3] {
    let upper = CLASSIC_STOPS.iter().position(|s| s.0 >= t).unwrap_or(CLASSIC_STOPS.len() - 1);
    if upper == 0 {
        return CLASSIC_STOPS[0].1;
    }
    let (a, b) = (CLASSIC_STOPS[upper - 1], CLASSIC_STOPS[upper]);
    let w = ((t - a.0) / (b.0 - a.0)).clamp(0.0, 1.0);
    std::array::from_fn(|c| (f32::from(a.1[c]) * (1.0 - w) + f32::from(b.1[c]) * w).round() as u8)
}

impl WaveformPalette {
    /// From the application's global LUT: `WaveformPalette::from_rgba(&lut.colors)`. An empty slice
    /// gives the default palette.
    pub fn from_rgba(colors: &[[u8; 4]]) -> Self {
        if colors.is_empty() {
            return Self::default();
        }
        Self { colors: colors.iter().map(|c| Color32::from_rgb(c[0], c[1], c[2])).collect() }
    }

    /// `t` is clamped to 0..=1; NaN gives the weakest colour.
    pub fn color(&self, t: f32) -> Color32 {
        let t = if t.is_nan() { 0.0 } else { t.clamp(0.0, 1.0) };
        self.colors[(t * (self.colors.len() - 1) as f32).round() as usize]
    }

    pub fn background(&self) -> Color32 {
        self.colors[0]
    }

    pub fn centre_line(&self) -> Color32 {
        self.color(CENTRE_LINE_LEVEL)
    }
}

/// Moves each channel `amount` (0..=1) of the way towards white: 0.05 on black gives rgb(13, 13, 13).
pub fn lighten(color: Color32, amount: f32) -> Color32 {
    let amount = if amount.is_nan() { 0.0 } else { amount.clamp(0.0, 1.0) };
    let up = |c: u8| (f32::from(c) + (255.0 - f32::from(c)) * amount).round() as u8;
    Color32::from_rgb(up(color.r()), up(color.g()), up(color.b()))
}

fn sanitise(v: f32) -> f32 {
    if v.is_finite() { v.clamp(-1.0, 1.0) } else { 0.0 }
}

/// Loudness of each column (largest absolute peak) after the gamma, 0 for silent columns.
pub fn levels(peaks: &[(f32, f32, f32)]) -> Vec<f32> {
    peaks
        .iter()
        .map(|&(min, max, _)| sanitise(min).abs().max(sanitise(max).abs()).powf(1.0 / WAVEFORM_GAMMA))
        .collect()
}

/// Moving average over a window of `2 * (pixels / 2) + 1` columns, counting only non-silent ones,
/// so notes do not fade into the silence around them. 0 or 1 disables it.
pub fn smooth_levels(levels: &[f32], pixels: usize) -> Vec<f32> {
    if pixels <= 1 {
        return levels.to_vec();
    }
    let radius = pixels / 2;
    let mut sums = vec![0.0f64];
    let mut counts = vec![0usize];
    for &level in levels {
        sums.push(sums[sums.len() - 1] + f64::from(level));
        counts.push(counts[counts.len() - 1] + usize::from(level > 0.0));
    }
    (0..levels.len())
        .map(|x| {
            if levels[x] <= 0.0 {
                return 0.0;
            }
            let (lo, hi) = (x.saturating_sub(radius), (x + radius + 1).min(levels.len()));
            ((sums[hi] - sums[lo]) / (counts[hi] - counts[lo]) as f64) as f32
        })
        .collect()
}

/// 1 on the centre line, `EDGE_SCALE` at the tip of the envelope.
pub fn fade(distance: f32) -> f32 {
    1.0 - (1.0 - EDGE_SCALE) * distance.clamp(0.0, 1.0)
}

fn gradient_quad(mesh: &mut Mesh, rect: Rect, top: Color32, bottom: Color32) {
    let base = mesh.vertices.len() as u32;
    mesh.colored_vertex(rect.left_top(), top);
    mesh.colored_vertex(rect.right_top(), top);
    mesh.colored_vertex(rect.right_bottom(), bottom);
    mesh.colored_vertex(rect.left_bottom(), bottom);
    mesh.add_triangle(base, base + 1, base + 2);
    mesh.add_triangle(base, base + 2, base + 3);
}

/// One column per peak `(min, max, rms)` across `wave`; `ppp` is the number of physical pixels per
/// point so that each column is one physical pixel wide. The background is the palette's lowest
/// colour, painted by the caller.
pub fn build_mesh(wave: Rect, ppp: f32, peaks: &[(f32, f32, f32)], palette: &WaveformPalette) -> Mesh {
    let mut mesh = Mesh::default();
    let smoothed = smooth_levels(&levels(peaks), COLOR_SMOOTHING_PX);
    let (mid, half) = (wave.center().y, (wave.height() / 2.0 - 1.0).max(1.0));
    let one_px = 1.0 / ppp;
    let centre = Rect::from_min_max(
        pos2(wave.left(), mid - one_px / 2.0),
        pos2(wave.right(), mid + one_px / 2.0),
    );
    mesh.add_colored_rect(centre, palette.centre_line());

    for (i, &raw) in peaks.iter().enumerate() {
        let (min, max, rms) = (sanitise(raw.0), sanitise(raw.1), sanitise(raw.2).abs());
        let (low, high) = (min.min(max).min(0.0), min.max(max).max(0.0));
        if smoothed[i] <= 0.0 || (low == 0.0 && high == 0.0) {
            continue;
        }
        let x = wave.left() + i as f32 * one_px;
        let envelope_level = smoothed[i] * ENVELOPE_SHARE;
        let body_level = (smoothed[i] + BODY_LIFT).min(1.0);
        let at = |level: f32, distance: f32| palette.color(level * fade(distance));
        let col = |top: f32, bottom: f32| Rect::from_min_max(pos2(x, top), pos2(x + one_px, bottom));

        let (top, bottom) = (mid - high * half, mid - low * half);
        let reach = |amplitude: f32, side: f32| if side <= f32::EPSILON { 1.0 } else { (amplitude / side).min(1.0) };
        if high > 0.0 {
            gradient_quad(&mut mesh, col(top, mid), at(envelope_level, 1.0), at(envelope_level, 0.0));
        }
        if low < 0.0 {
            gradient_quad(&mut mesh, col(mid, bottom), at(envelope_level, 0.0), at(envelope_level, 1.0));
        }
        let (body_up, body_down) = (rms.min(high), rms.min(-low));
        if body_up > 0.0 {
            let tip = at(body_level, reach(body_up, high));
            gradient_quad(&mut mesh, col(mid - body_up * half, mid), tip, at(body_level, 0.0));
        }
        if body_down > 0.0 {
            let tip = at(body_level, reach(body_down, -low));
            gradient_quad(&mut mesh, col(mid, mid + body_down * half), at(body_level, 0.0), tip);
        }
    }
    mesh
}

#[cfg(test)]
mod tests {
    use super::*;

    fn grey() -> WaveformPalette {
        let colors: Vec<[u8; 4]> = (0..=255u8).map(|v| [v, v, v, 255]).collect();
        WaveformPalette::from_rgba(&colors)
    }

    fn wave() -> Rect {
        Rect::from_min_max(pos2(0.0, 0.0), pos2(100.0, 40.0))
    }

    #[test]
    fn default_palette_is_the_classic_gradient() {
        let p = WaveformPalette::default();
        assert_eq!(p.color(0.0), Color32::from_rgb(0, 0, 0));
        assert_eq!(p.color(1.0), Color32::from_rgb(255, 235, 0));
        let stop = p.color(0.3);
        assert!(stop.g().abs_diff(20) <= 2 && stop.b().abs_diff(140) <= 2 && stop.r() == 0, "{stop:?}");
        assert_eq!(p.background(), p.color(0.0));
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
}
