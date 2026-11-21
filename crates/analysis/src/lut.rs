//! Colour palettes: 256-entry lookup tables loaded from horizontal-gradient PNG files.

use std::path::Path;

use crate::error::AnalysisError;

pub const LUT_SIZE: usize = 256;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Lut {
    pub name: String,
    /// Index 0 is the weakest value, 255 the strongest.
    pub colors: [[u8; 4]; LUT_SIZE],
}

const CLASSIC_STOPS: [(f32, [u8; 3]); 4] = [
    (0.0, [0, 0, 0]),
    (0.3, [0, 20, 140]),
    (0.65, [0, 190, 70]),
    (1.0, [255, 235, 0]),
];

impl Lut {
    pub fn from_stops(name: &str, stops: &[(f32, [u8; 3])]) -> Self {
        let mut colors = [[0, 0, 0, 255]; LUT_SIZE];
        for (i, color) in colors.iter_mut().enumerate() {
            let t = i as f32 / (LUT_SIZE - 1) as f32;
            *color = gradient_at(stops, t);
        }
        Self {
            name: name.to_owned(),
            colors,
        }
    }

    pub fn builtin() -> Self {
        Self::from_stops("Classic", &CLASSIC_STOPS)
    }

    /// `t` is clamped to 0..=1; NaN maps to the weakest colour.
    pub fn color(&self, t: f32) -> [u8; 4] {
        let t = if t.is_nan() { 0.0 } else { t.clamp(0.0, 1.0) };
        self.colors[(t * (LUT_SIZE - 1) as f32).round() as usize]
    }

    pub fn from_png(path: &Path) -> Result<Self, AnalysisError> {
        let image = image::open(path)?.to_rgba8();
        let (width, height) = image.dimensions();
        if width == 0 || height == 0 {
            return Err(AnalysisError::InvalidPalette(format!(
                "{} is empty",
                path.display()
            )));
        }
        let y = height / 2;
        let pixel = |x: u32| image.get_pixel(x, y).0;
        let mut colors = [[0, 0, 0, 255]; LUT_SIZE];
        for (i, color) in colors.iter_mut().enumerate() {
            let pos = i as f32 / (LUT_SIZE - 1) as f32 * (width - 1) as f32;
            let x0 = pos.floor() as u32;
            let x1 = (x0 + 1).min(width - 1);
            let w = pos - x0 as f32;
            let (a, b) = (pixel(x0), pixel(x1));
            let rgb = mix([a[0], a[1], a[2]], [b[0], b[1], b[2]], w);
            *color = [rgb[0], rgb[1], rgb[2], 255];
        }
        let name = path
            .file_stem()
            .map_or_else(String::new, |s| s.to_string_lossy().into_owned());
        Ok(Self { name, colors })
    }
}

fn mix(a: [u8; 3], b: [u8; 3], w: f32) -> [u8; 3] {
    std::array::from_fn(|c| (f32::from(a[c]) * (1.0 - w) + f32::from(b[c]) * w).round() as u8)
}

fn gradient_at(stops: &[(f32, [u8; 3])], t: f32) -> [u8; 4] {
    let (Some(first), Some(last)) = (stops.first(), stops.last()) else {
        return [0, 0, 0, 255];
    };
    let rgb = if t <= first.0 {
        first.1
    } else if t >= last.0 {
        last.1
    } else {
        let upper = stops
            .iter()
            .position(|s| s.0 >= t)
            .unwrap_or(stops.len() - 1)
            .max(1);
        let (a, b) = (stops[upper - 1], stops[upper]);
        let w = (t - a.0) / (b.0 - a.0).max(f32::EPSILON);
        mix(a.1, b.1, w)
    };
    [rgb[0], rgb[1], rgb[2], 255]
}

/// Unreadable files are skipped with a warning; a missing folder yields an empty list.
pub fn discover_luts(dir: &Path) -> Vec<Lut> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut luts: Vec<Lut> = entries
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| {
            path.is_file()
                && path
                    .extension()
                    .is_some_and(|e| e.eq_ignore_ascii_case("png"))
        })
        .filter_map(|path| match Lut::from_png(&path) {
            Ok(lut) => Some(lut),
            Err(err) => {
                log::warn!("skipping palette {}: {err}", path.display());
                None
            }
        })
        .collect();
    luts.sort_by(|a, b| a.name.cmp(&b.name));
    luts
}

#[cfg(all(test, dev_tests))]
#[path = "../../../dev/tests/unit/analysis/lut.rs"]
mod tests;

#[cfg(all(test, dev_tests))]
#[path = "../../../dev/tests/unit/analysis/lut_support.rs"]
mod test_support;
