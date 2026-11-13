//! Background autosave and crash-recovery decisions.

use std::io;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{channel, Receiver, Sender};
use std::thread::JoinHandle;
use std::time::SystemTime;

use fretscribe_core::{discard_autosave, save_autosave, ProjectFile};
use uuid::Uuid;

const MARKER: &str = "session.marker";
const RUNNING: &str = "running";

pub fn autosave_path(dir: &Path, project_id: Uuid) -> PathBuf {
    dir.join("autosave").join(format!("{project_id}.autosave"))
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Recovery {
    None,
    OfferRestore,
}

pub fn decide_recovery(
    autosave_modified: Option<SystemTime>,
    project_file_modified: Option<SystemTime>,
    project_ever_saved: bool,
    clean_exit: bool,
) -> Recovery {
    let newer = match (autosave_modified, project_file_modified) {
        (Some(a), Some(f)) => a > f,
        (Some(_), None) => !project_ever_saved,
        _ => false,
    };
    if autosave_modified.is_some() && !clean_exit && (!project_ever_saved || newer) {
        Recovery::OfferRestore
    } else {
        Recovery::None
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AutosavePolicy {
    pub enabled: bool,
    pub interval_secs: u32,
    pub every_edits: u32,
}

impl AutosavePolicy {
    pub fn should_autosave(&self, unsaved: bool, secs_since_last: f64, edits_since_last: u32) -> bool {
        self.enabled
            && unsaved
            && edits_since_last > 0
            && (secs_since_last >= f64::from(self.interval_secs) || edits_since_last >= self.every_edits)
    }
}

pub struct AutosaveWorker {
    tx: Option<Sender<(ProjectFile, PathBuf)>>,
    errors: Receiver<String>,
    thread: Option<JoinHandle<()>>,
}

fn write(file: &ProjectFile, path: &Path) -> Result<(), String> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }
    save_autosave(file, path).map_err(|e| e.to_string())
}

impl AutosaveWorker {
    pub fn spawn() -> Self {
        let (tx, rx) = channel::<(ProjectFile, PathBuf)>();
        let (err_tx, errors) = channel();
        let thread = std::thread::spawn(move || {
            while let Ok(mut job) = rx.recv() {
                while let Ok(newer) = rx.try_recv() {
                    job = newer;
                }
                if let Err(e) = write(&job.0, &job.1) {
                    let _ = err_tx.send(format!("autosave to {} failed: {e}", job.1.display()));
                }
            }
        });
        Self { tx: Some(tx), errors, thread: Some(thread) }
    }

    /// Jobs queued while one is being written collapse to the latest.
    pub fn submit(&self, file: ProjectFile, path: PathBuf) {
        if let Some(tx) = &self.tx {
            let _ = tx.send((file, path));
        }
    }

    pub fn try_recv_error(&self) -> Option<String> {
        self.errors.try_recv().ok()
    }
}

impl Drop for AutosaveWorker {
    fn drop(&mut self) {
        self.tx.take();
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

pub fn discard(path: &Path) -> io::Result<()> {
    discard_autosave(path)
}

fn write_marker(dir: &Path, content: &str) -> io::Result<()> {
    std::fs::create_dir_all(dir)?;
    std::fs::write(dir.join(MARKER), content)
}

pub fn mark_running(dir: &Path) -> io::Result<()> {
    write_marker(dir, RUNNING)
}

pub fn mark_clean_exit(dir: &Path) -> io::Result<()> {
    write_marker(dir, "clean")
}

/// Call before `mark_running`; a missing marker (first run) counts as clean.
pub fn was_clean_exit(dir: &Path) -> bool {
    std::fs::read_to_string(dir.join(MARKER)).map_or(true, |s| s != RUNNING)
}

#[cfg(test)]
mod tests {
    use super::*;
    use fretscribe_core::{load_autosave, Project, TimeSignature, Track, TrackRole};
    use std::time::Duration;

    fn file() -> ProjectFile {
        ProjectFile::new(Project::new(Track::new(1, "t", TrackRole::Guitar), 10.0, 120.0, TimeSignature::COMMON))
    }

    #[test]
    fn recovery_decision_table() {
        let old = SystemTime::UNIX_EPOCH + Duration::from_secs(100);
        let new = old + Duration::from_secs(50);
        let d = decide_recovery;
        assert_eq!(d(Some(new), Some(old), true, false), Recovery::OfferRestore);
        assert_eq!(d(Some(old), Some(new), true, false), Recovery::None);
        assert_eq!(d(Some(new), None, false, false), Recovery::OfferRestore);
        assert_eq!(d(Some(old), Some(new), false, false), Recovery::OfferRestore);
        assert_eq!(d(Some(new), Some(old), true, true), Recovery::None);
        assert_eq!(d(None, Some(old), true, false), Recovery::None);
        assert_eq!(d(None, None, false, false), Recovery::None);
        assert_eq!(d(Some(new), None, true, false), Recovery::None);
    }

    #[test]
    fn policy_thresholds() {
        let p = AutosavePolicy { enabled: true, interval_secs: 30, every_edits: 20 };
        assert!(!p.should_autosave(false, 999.0, 99));
        assert!(!p.should_autosave(true, 999.0, 0));
        assert!(!p.should_autosave(true, 29.9, 19));
        assert!(p.should_autosave(true, 30.0, 1));
        assert!(p.should_autosave(true, 1.0, 20));
        assert!(!AutosavePolicy { enabled: false, ..p }.should_autosave(true, 999.0, 99));
    }

    #[test]
    fn worker_writes_generations_without_leftovers() {
        let dir = tempfile::tempdir().unwrap();
        let f = file();
        let path = autosave_path(dir.path(), f.project_id);
        assert!(path.ends_with(format!("autosave/{}.autosave", f.project_id)));
        for _ in 0..3 {
            let worker = AutosaveWorker::spawn();
            worker.submit(f.clone(), path.clone());
        }
        assert_eq!(load_autosave(&path).unwrap().map(|l| l.project_id), Some(f.project_id));
        let names: Vec<String> = std::fs::read_dir(path.parent().unwrap())
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        assert_eq!(names.len(), 3, "{names:?}");
        assert!(names.iter().all(|n| !n.ends_with(".tmp")));
        discard(&path).unwrap();
        assert!(!path.exists());
    }

    #[test]
    fn worker_reports_unwritable_path() {
        let dir = tempfile::tempdir().unwrap();
        let blocker = dir.path().join("file");
        std::fs::write(&blocker, b"x").unwrap();
        let worker = AutosaveWorker::spawn();
        worker.submit(file(), blocker.join("sub/a.autosave"));
        let deadline = std::time::Instant::now() + Duration::from_secs(5);
        let error = loop {
            if let Some(e) = worker.try_recv_error() {
                break e;
            }
            assert!(std::time::Instant::now() < deadline, "no error reported");
            std::thread::sleep(Duration::from_millis(10));
        };
        assert!(error.contains("autosave"));
    }

    #[test]
    fn session_marker() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path();
        assert!(was_clean_exit(dir));
        mark_running(dir).unwrap();
        assert!(!was_clean_exit(dir));
        mark_clean_exit(dir).unwrap();
        assert!(was_clean_exit(dir));
    }
}
