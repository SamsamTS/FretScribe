//! Colour palettes: 256-entry lookup tables loaded from horizontal-gradient SVG files.

use std::path::Path;
use std::sync::LazyLock;

use resvg::usvg::{self, TreeParsing};
use resvg::{tiny_skia, Tree};

use crate::error::AnalysisError;

pub const LUT_SIZE: usize = 256;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Lut {
    pub name: String,
    /// Index 0 is the weakest value, 255 the strongest.
    pub colors: [[u8; 4]; LUT_SIZE],
}

macro_rules! embedded {
    ($($name:literal),+) => {
        [$(($name, include_bytes!(concat!("../../../LUT/", $name, ".svg")).as_slice())),+]
    };
}

/// Name and SVG file content of each embedded palette, the default one first.
pub const BUILTIN_FILES: [(&str, &[u8]); 6] =
    embedded!("Heat", "Cool", "Amber", "Aurora", "Ice", "Nebula");

static BUILTINS: LazyLock<[Lut; BUILTIN_FILES.len()]> = LazyLock::new(|| {
    BUILTIN_FILES.map(|(name, data)| {
        // A unit test guarantees the embedded files parse; the black fallback is never used.
        Lut::from_svg(name, data).unwrap_or_else(|_| Lut {
            name: name.to_owned(),
            colors: [[0, 0, 0, 255]; LUT_SIZE],
        })
    })
});

impl Lut {
    /// The embedded palettes, the default one first.
    pub fn builtins() -> &'static [Self] {
        BUILTINS.as_slice()
    }

    /// The default palette.
    pub fn builtin() -> Self {
        BUILTINS[0].clone()
    }

    /// `t` is clamped to 0..=1; NaN maps to the weakest colour.
    pub fn color(&self, t: f32) -> [u8; 4] {
        let t = if t.is_nan() { 0.0 } else { t.clamp(0.0, 1.0) };
        self.colors[(t * (LUT_SIZE - 1) as f32).round() as usize]
    }

    /// A horizontal gradient filling the whole image, weakest on the left.
    pub fn from_svg(name: &str, data: &[u8]) -> Result<Self, AnalysisError> {
        let invalid = |reason: String| AnalysisError::InvalidPalette(format!("{name}: {reason}"));
        let tree = usvg::Tree::from_data(data, &usvg::Options::default())
            .map_err(|e| invalid(e.to_string()))?;
        let tree = Tree::from_usvg(&tree);
        let mut pixmap = tiny_skia::Pixmap::new(LUT_SIZE as u32, 1)
            .ok_or_else(|| invalid("cannot allocate the image".to_owned()))?;
        let scale = tiny_skia::Transform::from_scale(
            LUT_SIZE as f32 / tree.size.width(),
            1.0 / tree.size.height(),
        );
        tree.render(scale, &mut pixmap.as_mut());
        let mut colors = [[0, 0, 0, 255]; LUT_SIZE];
        for (color, pixel) in colors.iter_mut().zip(pixmap.data().chunks_exact(4)) {
            color[..3].copy_from_slice(&pixel[..3]);
        }
        Ok(Self {
            name: name.to_owned(),
            colors,
        })
    }

    fn from_file(path: &Path) -> Result<Self, AnalysisError> {
        let name = path.file_stem().unwrap_or_default().to_string_lossy();
        Self::from_svg(&name, &std::fs::read(path)?)
    }
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
                    .is_some_and(|e| e.eq_ignore_ascii_case("svg"))
        })
        .filter_map(|path| match Lut::from_file(&path) {
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
