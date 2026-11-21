//! Linear undo/redo history of `ProjectEdit`s.
//!
//! Old history is thinned on a log scale (count based, no clock): the newest `min_entries` entries
//! are never merged; older ones have non-decreasing power-of-two weights, at most `per_level` per
//! weight, so the size is `O(min_entries + per_level * log N)` for any number of edits.

use std::collections::VecDeque;

use serde::{Deserialize, Serialize};

use crate::edit::ProjectEdit;
use crate::error::UndoError;
use crate::model::Project;

pub const DEFAULT_UNDO_LIMIT: usize = 200;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct UndoPolicy {
    pub min_entries: usize,
    pub per_level: usize,
    pub max_entries: usize,
}

impl Default for UndoPolicy {
    fn default() -> Self {
        Self {
            min_entries: 20,
            per_level: 3,
            max_entries: DEFAULT_UNDO_LIMIT,
        }
    }
}

impl UndoPolicy {
    /// `per_level >= 1`, `max_entries >= 1`, and `min_entries < max_entries` (lowered if needed).
    fn sanitised(self) -> Self {
        let max_entries = self.max_entries.max(1);
        Self {
            min_entries: self.min_entries.min(max_entries - 1),
            per_level: self.per_level.max(1),
            max_entries,
        }
    }
}

/// The done stack, oldest to newest (the redo stack is not saved).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SavedHistory {
    done: Vec<ProjectEdit>,
}

/// Pushing a new edit discards the redo branch.
#[derive(Clone, Debug, Default)]
pub struct UndoStack {
    done: VecDeque<ProjectEdit>,
    undone: Vec<ProjectEdit>,
    policy: UndoPolicy,
    diverged: bool,
}

impl UndoStack {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_policy(policy: UndoPolicy) -> Self {
        Self {
            policy: policy.sanitised(),
            ..Self::default()
        }
    }

    pub fn with_limit(limit: usize) -> Self {
        Self::with_policy(UndoPolicy {
            max_entries: limit,
            ..UndoPolicy::default()
        })
    }

    /// Takes effect at the next push.
    pub fn set_policy(&mut self, policy: UndoPolicy) {
        self.policy = policy.sanitised();
    }

    pub fn push(&mut self, edit: ProjectEdit) {
        self.undone.clear();
        self.done.push_back(edit);
        self.enforce();
    }

    /// Like `push`, but folds the edit into the latest one so both undo as a single step.
    pub fn push_merged(&mut self, edit: ProjectEdit) {
        self.undone.clear();
        match self.done.pop_back() {
            Some(last) => self.done.push_back(last.merge(edit)),
            None => self.done.push_back(edit),
        }
    }

    fn enforce(&mut self) {
        let UndoPolicy {
            min_entries,
            per_level,
            max_entries,
        } = self.policy;
        // A weight level that overflows merges its two oldest entries into the next level.
        loop {
            let mut end = self.done.len().saturating_sub(min_entries);
            let overflow = loop {
                let Some(w) = end.checked_sub(1).map(|i| self.done[i].weight()) else {
                    break None;
                };
                let start = (0..end)
                    .rev()
                    .take_while(|&i| self.done[i].weight() == w)
                    .last()
                    .unwrap_or(end);
                if end - start > per_level {
                    break Some(start);
                }
                end = start;
            };
            let Some(start) = overflow else { break };
            let (Some(later), Some(first)) = (self.done.remove(start + 1), self.done.remove(start))
            else {
                break;
            };
            let weight = first.weight() + later.weight();
            self.done.insert(start, first.merge_weighted(later, weight));
        }
        while self.done.len() > max_entries {
            self.done.pop_front();
        }
    }

    pub fn can_undo(&self) -> bool {
        !self.done.is_empty()
    }

    pub fn can_redo(&self) -> bool {
        !self.undone.is_empty()
    }

    pub fn undo_label(&self) -> Option<&str> {
        self.done.back().map(ProjectEdit::label)
    }

    pub fn redo_label(&self) -> Option<&str> {
        self.undone.last().map(ProjectEdit::label)
    }

    /// Steps of the edit `undo` would revert (0 when there is none).
    pub fn undo_weight(&self) -> usize {
        self.done.back().map_or(0, ProjectEdit::weight)
    }

    pub fn redo_weight(&self) -> usize {
        self.undone.last().map_or(0, ProjectEdit::weight)
    }

    /// Returns the edit that was reverted, `None` when there is nothing to undo or the project
    /// diverged from the history (which is then cleared, see `take_diverged`).
    pub fn undo(&mut self, project: &mut Project) -> Option<&ProjectEdit> {
        let edit = self.done.pop_back()?;
        self.run(edit, project, true)
    }

    /// Returns the edit that was re-applied, with the same caveats as `undo`.
    pub fn redo(&mut self, project: &mut Project) -> Option<&ProjectEdit> {
        let edit = self.undone.pop()?;
        self.run(edit, project, false)
    }

    fn run(
        &mut self,
        edit: ProjectEdit,
        project: &mut Project,
        undo: bool,
    ) -> Option<&ProjectEdit> {
        let result = if undo {
            edit.try_undo(project)
        } else {
            edit.try_redo(project)
        };
        if result == Err(UndoError::Diverged) {
            self.clear();
            self.diverged = true;
            return None;
        }
        if undo {
            self.undone.push(edit);
            self.undone.last()
        } else {
            self.done.push_back(edit);
            self.done.back()
        }
    }

    /// True once after a divergence cleared the history, so the app can show a toast.
    pub fn take_diverged(&mut self) -> bool {
        std::mem::take(&mut self.diverged)
    }

    pub fn clear(&mut self) {
        self.done.clear();
        self.undone.clear();
    }

    pub fn to_saved(&self) -> Option<SavedHistory> {
        (!self.done.is_empty()).then(|| SavedHistory {
            done: self.done.iter().cloned().collect(),
        })
    }

    /// Never fails: replays the entries backwards from `current` and keeps the newest run that still
    /// applies and leaves a valid project.
    pub fn from_saved(saved: SavedHistory, current: &Project, policy: UndoPolicy) -> Self {
        let mut scratch = current.clone();
        let mut kept: Vec<ProjectEdit> = saved
            .done
            .into_iter()
            .rev()
            .take_while(|edit| edit.try_undo(&mut scratch).is_ok() && scratch.validate().is_ok())
            .collect();
        kept.reverse();
        let mut stack = Self::with_policy(policy);
        stack.done = kept.into();
        stack.enforce();
        stack
    }
}

#[cfg(all(test, dev_tests))]
#[path = "../../../dev/tests/unit/core/undo.rs"]
mod tests;
