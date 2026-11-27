//! LUT-coloured waveform for the overview strip. It reproduces the colouring of
//! `fretscribe_analysis::waveform::render_rgba` on egui meshes.

use egui::{pos2, Color32, Mesh, Rect};
use fretscribe_analysis::waveform::{
    BODY_LIFT, CENTRE_LINE_LEVEL, EDGE_SCALE, ENVELOPE_SHARE, WAVEFORM_COLOR_SMOOTHING_PX,
};
use fretscribe_analysis::Lut;
pub use fretscribe_analysis::WAVEFORM_GAMMA;

/// 256 colours, index 0 the weakest and 255 the strongest, like `analysis::lut::Lut::colors`.
#[derive(Clone, Debug, PartialEq)]
pub struct WaveformPalette {
    colors: Vec<Color32>,
}

impl Default for WaveformPalette {
    /// The default built-in palette.
    fn default() -> Self {
        Self::from_rgba(&Lut::builtin().colors)
    }
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
pub(crate) fn levels(peaks: &[(f32, f32, f32)]) -> Vec<f32> {
    peaks
        .iter()
        .map(|&(min, max, _)| sanitise(min).abs().max(sanitise(max).abs()).powf(1.0 / WAVEFORM_GAMMA))
        .collect()
}

/// Moving average over a window of `2 * (pixels / 2) + 1` columns, counting only non-silent ones,
/// so notes do not fade into the silence around them. 0 or 1 disables it.
pub(crate) fn smooth_levels(levels: &[f32], pixels: usize) -> Vec<f32> {
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
pub(crate) fn fade(distance: f32) -> f32 {
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
pub(crate) fn build_mesh(wave: Rect, ppp: f32, peaks: &[(f32, f32, f32)], palette: &WaveformPalette) -> Mesh {
    let mut mesh = Mesh::default();
    let smoothed = smooth_levels(&levels(peaks), WAVEFORM_COLOR_SMOOTHING_PX);
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

#[cfg(all(test, dev_tests))]
#[path = "../../../../dev/tests/unit/ui/overview/waveform.rs"]
mod tests;
