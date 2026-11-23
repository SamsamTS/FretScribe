//! Spectrogram palettes discovered on disk, with the built-in one as fallback.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use fretscribe_analysis::{discover_luts, Lut};

#[derive(Clone)]
pub struct LutSet {
    pub luts: Vec<Arc<Lut>>,
    names: Vec<String>,
    pub dir: Option<PathBuf>,
    pub builtin_fallback: bool,
}

pub fn lut_dir(exe_dir: &Path, workspace_root: &Path) -> Option<PathBuf> {
    [exe_dir, workspace_root]
        .into_iter()
        .map(|d| d.join("LUT"))
        .find(|d| d.is_dir())
}

impl LutSet {
    pub fn scan(dir: Option<&Path>) -> Self {
        let mut luts: Vec<Arc<Lut>> = dir.map(discover_luts).unwrap_or_default().into_iter().map(Arc::new).collect();
        let builtin_fallback = luts.is_empty();
        if builtin_fallback {
            luts.push(Arc::new(Lut::builtin()));
        }
        Self {
            names: luts.iter().map(|l| l.name.clone()).collect(),
            luts,
            dir: dir.map(Path::to_path_buf),
            builtin_fallback,
        }
    }

    pub fn index_of(&self, name: &str) -> usize {
        self.luts.iter().position(|l| l.name == name).unwrap_or(0)
    }

    pub fn names(&self) -> &[String] {
        &self.names
    }
}

#[cfg(all(test, dev_tests))]
#[path = "../../../dev/tests/unit/app/luts.rs"]
mod tests;
