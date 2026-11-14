//! Spectrogram palettes discovered on disk, with the built-in one as fallback.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use fretscribe_analysis::{discover_luts, Lut};

#[derive(Clone)]
pub struct LutSet {
    pub luts: Vec<Arc<Lut>>,
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
            luts,
            dir: dir.map(Path::to_path_buf),
            builtin_fallback,
        }
    }

    pub fn index_of(&self, name: &str) -> usize {
        self.luts.iter().position(|l| l.name == name).unwrap_or(0)
    }

    pub fn names(&self) -> Vec<&str> {
        self.luts.iter().map(|l| l.name.as_str()).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write_lut(dir: &Path, name: &str, rgb: [u8; 3]) {
        image::RgbImage::from_pixel(256, 1, image::Rgb(rgb)).save(dir.join(name)).unwrap();
    }

    #[test]
    fn lut_dir_prefers_exe_then_workspace() {
        let (exe, ws) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
        assert_eq!(lut_dir(exe.path(), ws.path()), None);
        std::fs::create_dir(ws.path().join("LUT")).unwrap();
        assert_eq!(lut_dir(exe.path(), ws.path()), Some(ws.path().join("LUT")));
        std::fs::create_dir(exe.path().join("LUT")).unwrap();
        assert_eq!(lut_dir(exe.path(), ws.path()), Some(exe.path().join("LUT")));
    }

    #[test]
    fn scan_falls_back_to_builtin() {
        for dir in [None, Some(tempfile::tempdir().unwrap())] {
            let set = LutSet::scan(dir.as_ref().map(|d| d.path()));
            assert!(set.builtin_fallback);
            assert_eq!(set.names(), ["Classic"]);
        }
        assert!(LutSet::scan(Some(Path::new("/definitely/missing"))).builtin_fallback);
    }

    #[test]
    fn scan_lists_and_indexes_by_name() {
        let dir = tempfile::tempdir().unwrap();
        write_lut(dir.path(), "Fire.png", [200, 0, 0]);
        write_lut(dir.path(), "Ice.png", [0, 0, 200]);
        let set = LutSet::scan(Some(dir.path()));
        assert!(!set.builtin_fallback);
        assert_eq!(set.names().len(), 2);
        let ice = set.names()[1].to_owned();
        assert_eq!(set.index_of(&ice), 1);
        assert_eq!(set.index_of("nope"), 0);
    }
}
