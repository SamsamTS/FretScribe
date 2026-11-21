//! Project-relative audio paths.

use std::path::{Component, Path, PathBuf};

use fretscribe_core::{Project, Track};

fn parts(p: &Path) -> Vec<Component<'_>> {
    p.components().filter(|c| *c != Component::CurDir).collect()
}

/// Both paths must share a root or prefix; `None` otherwise (or when `base_dir` climbs with `..`).
pub fn relative_to(base_dir: &Path, target: &Path) -> Option<PathBuf> {
    let (base, target) = (parts(base_dir), parts(target));
    let anchor = |v: &[Component]| v.iter().take_while(|c| matches!(c, Component::Prefix(_) | Component::RootDir)).count();
    let common = base.iter().zip(&target).take_while(|(a, b)| a == b).count();
    if anchor(&base) != anchor(&target) || common < anchor(&base) {
        return None;
    }
    let rest = &base[common..];
    if rest.iter().any(|c| matches!(c, Component::ParentDir)) {
        return None;
    }
    let mut out: PathBuf = rest.iter().map(|_| Component::ParentDir).collect();
    out.extend(&target[common..]);
    Some(out)
}

pub fn resolve_audio(project_file: Option<&Path>, track: &Track) -> Option<PathBuf> {
    let relative = project_file
        .and_then(Path::parent)
        .zip(track.audio_rel_path.as_deref())
        .map(|(dir, rel)| dir.join(rel));
    relative
        .into_iter()
        .chain(track.audio_path.clone())
        .find(|p| p.exists())
}

pub fn fill_relative_paths(project: &mut Project, project_file: &Path) {
    let base = project_file.parent().unwrap_or_else(|| Path::new(""));
    for track in &mut project.tracks {
        track.audio_rel_path = track.audio_path.as_deref().and_then(|p| relative_to(base, p));
    }
}

#[cfg(all(test, dev_tests))]
#[path = "../../../dev/tests/unit/app/paths.rs"]
mod tests;
