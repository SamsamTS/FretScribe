//! Spectrogram palettes: the built-in ones, then those discovered on disk.

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
        let mut found = dir.map(discover_luts).unwrap_or_default();
        let builtin_fallback = found.is_empty();
        let builtins = Lut::builtins();
        found.retain_mut(|lut| {
            let Some(builtin) = builtins.iter().find(|b| b.name.eq_ignore_ascii_case(&lut.name)) else {
                return true;
            };
            lut.name = format!("{} (Custom)", builtin.name);
            lut.colors != builtin.colors
        });
        found.sort_by(|a, b| a.name.cmp(&b.name));
        let luts: Vec<Arc<Lut>> = builtins.iter().cloned().chain(found).map(Arc::new).collect();
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
