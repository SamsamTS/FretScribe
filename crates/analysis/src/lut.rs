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

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use image::{Rgba, RgbaImage};

    use super::*;

    fn lut_dir() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../LUT")
    }

    fn write_gradient(path: &Path, width: u32, left: [u8; 3], right: [u8; 3]) {
        let image = RgbaImage::from_fn(width, 3, |x, _| {
            let w = x as f32 / (width - 1).max(1) as f32;
            let mut px = [0u8, 0, 0, 77];
            for c in 0..3 {
                px[c] = (f32::from(left[c]) * (1.0 - w) + f32::from(right[c]) * w).round() as u8;
            }
            Rgba(px)
        });
        image.save(path).expect("write png");
    }

    #[test]
    fn from_png_resamples_to_256_and_forces_alpha() {
        let dir = tempfile::tempdir().expect("tempdir");
        for width in [2, 100, 256, 1000] {
            let path = dir.path().join(format!("Grad{width}.png"));
            write_gradient(&path, width, [10, 20, 30], [200, 150, 100]);
            let lut = Lut::from_png(&path).expect("load");
            assert_eq!(lut.name, format!("Grad{width}"));
            assert_eq!(lut.colors.len(), 256);
            assert_eq!(lut.colors[0], [10, 20, 30, 255]);
            assert_eq!(lut.colors[255], [200, 150, 100, 255]);
            assert!(lut.colors.iter().all(|c| c[3] == 255));
            let mid = lut.colors[128];
            assert!((i32::from(mid[0]) - 105).abs() <= 2, "{mid:?}");
        }
    }

    #[test]
    fn discover_returns_png_files_sorted_and_tolerates_problems() {
        let dir = tempfile::tempdir().expect("tempdir");
        write_gradient(&dir.path().join("Zeta.png"), 16, [0, 0, 0], [255, 255, 255]);
        write_gradient(&dir.path().join("Alpha.png"), 16, [0, 0, 0], [255, 0, 0]);
        std::fs::write(dir.path().join("notes.txt"), "not a palette").expect("write");
        std::fs::write(dir.path().join("Broken.png"), "not a png").expect("write");
        let names: Vec<String> = discover_luts(dir.path())
            .into_iter()
            .map(|l| l.name)
            .collect();
        assert_eq!(names, ["Alpha", "Zeta"]);
        assert!(discover_luts(&dir.path().join("missing")).is_empty());
    }

    #[test]
    fn repository_palettes_load() {
        let luts = discover_luts(&lut_dir());
        let names: Vec<&str> = luts.iter().map(|l| l.name.as_str()).collect();
        assert!(
            names.contains(&"Classic") && names.contains(&"Heat"),
            "{names:?}"
        );
        for lut in &luts {
            assert_eq!(lut.colors[0], [0, 0, 0, 255], "{}", lut.name);
        }
    }

    #[test]
    fn color_clamps_and_hits_both_ends() {
        let lut = Lut::builtin();
        assert_eq!(lut.color(0.0), lut.colors[0]);
        assert_eq!(lut.color(1.0), lut.colors[255]);
        assert_eq!(lut.color(-3.0), lut.colors[0]);
        assert_eq!(lut.color(7.0), lut.colors[255]);
        assert_eq!(lut.color(f32::NAN), lut.colors[0]);
        assert_eq!(lut.color(0.5), lut.colors[128]);
    }

    #[test]
    fn builtin_matches_classic_png() {
        let png = Lut::from_png(&lut_dir().join("Classic.png")).expect("Classic.png");
        assert_eq!(png, Lut::builtin());
    }
}
