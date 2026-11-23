//! Background autosave and crash-recovery decisions.

use std::io;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{channel, Receiver, Sender};
use std::thread::JoinHandle;
use std::time::SystemTime;

use fretscribe_core::{discard_autosave, save_autosave, ProjectFile};
use fretscribe_core::Uuid;

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

#[cfg(all(test, dev_tests))]
#[path = "../../../dev/tests/unit/app/autosave.rs"]
mod tests;
