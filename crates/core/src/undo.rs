//! Linear undo/redo history of `ProjectEdit`s.

use std::collections::VecDeque;

use crate::edit::ProjectEdit;
use crate::model::Project;

pub const DEFAULT_UNDO_LIMIT: usize = 200;

/// Pushing a new edit discards the redo branch; the oldest edits are dropped beyond the limit.
#[derive(Clone, Debug)]
pub struct UndoStack {
    done: VecDeque<ProjectEdit>,
    undone: Vec<ProjectEdit>,
    limit: usize,
}

impl Default for UndoStack {
    fn default() -> Self {
        Self::with_limit(DEFAULT_UNDO_LIMIT)
    }
}

impl UndoStack {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_limit(limit: usize) -> Self {
        Self {
            done: VecDeque::new(),
            undone: Vec::new(),
            limit: limit.max(1),
        }
    }

    pub fn push(&mut self, edit: ProjectEdit) {
        self.undone.clear();
        self.done.push_back(edit);
        while self.done.len() > self.limit {
            self.done.pop_front();
        }
    }

    /// Like `push`, but folds the edit into the latest one so both undo as a single step.
    pub fn push_merged(&mut self, edit: ProjectEdit) {
        self.undone.clear();
        match self.done.pop_back() {
            Some(last) => self.done.push_back(last.merge(edit)),
            None => self.done.push_back(edit),
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

    /// Returns the edit that was reverted, `None` when there is nothing to undo.
    pub fn undo(&mut self, project: &mut Project) -> Option<&ProjectEdit> {
        let edit = self.done.pop_back()?;
        edit.undo(project);
        self.undone.push(edit);
        self.undone.last()
    }

    /// Returns the edit that was re-applied, `None` when there is nothing to redo.
    pub fn redo(&mut self, project: &mut Project) -> Option<&ProjectEdit> {
        let edit = self.undone.pop()?;
        edit.redo(project);
        self.done.push_back(edit);
        self.done.back()
    }

    pub fn clear(&mut self) {
        self.done.clear();
        self.undone.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::edit::NotePolicy;
    use crate::model::{TimeSignature, Track, TrackRole};

    fn project() -> Project {
        Project::new(
            Track::new(1, "Guitar", TrackRole::Guitar),
            8.0,
            120.0,
            TimeSignature::COMMON,
        )
    }

    fn tempo_edit(p: &mut Project, bpm: f64) -> ProjectEdit {
        p.set_tempo(0, bpm, NotePolicy::FollowGrid).unwrap()
    }

    fn first_tempo(p: &Project) -> f64 {
        p.measures[0].tempo_bpm
    }

    #[test]
    fn empty_stack_does_nothing() {
        let mut p = project();
        let before = p.clone();
        let mut stack = UndoStack::new();
        assert!(!stack.can_undo() && !stack.can_redo());
        assert!(stack.undo(&mut p).is_none());
        assert!(stack.redo(&mut p).is_none());
        assert_eq!((stack.undo_label(), stack.redo_label()), (None, None));
        assert_eq!(p, before);
    }

    #[test]
    fn undo_and_redo_walk_the_history() {
        let mut p = project();
        let original = p.clone();
        let mut stack = UndoStack::new();
        let e1 = tempo_edit(&mut p, 100.0);
        stack.push(e1);
        let e2 = tempo_edit(&mut p, 80.0);
        stack.push(e2);
        assert_eq!(stack.undo_label(), Some("Set tempo"));

        assert_eq!(stack.undo(&mut p).map(ProjectEdit::label), Some("Set tempo"));
        assert_eq!(first_tempo(&p), 100.0);
        assert!(stack.can_redo());
        stack.undo(&mut p);
        assert_eq!(p, original);
        assert!(!stack.can_undo());

        stack.redo(&mut p);
        stack.redo(&mut p);
        assert_eq!(first_tempo(&p), 80.0);
        assert!(!stack.can_redo());
    }

    #[test]
    fn pushing_discards_the_redo_branch() {
        let mut p = project();
        let mut stack = UndoStack::new();
        let e = tempo_edit(&mut p, 100.0);
        stack.push(e);
        stack.undo(&mut p);
        assert!(stack.can_redo());
        let e = tempo_edit(&mut p, 90.0);
        stack.push(e);
        assert!(!stack.can_redo());
        assert_eq!(stack.redo_label(), None);
    }

    #[test]
    fn oldest_edits_are_dropped_beyond_the_limit() {
        let mut p = project();
        let mut stack = UndoStack::with_limit(2);
        for bpm in [110.0, 100.0, 90.0] {
            let e = tempo_edit(&mut p, bpm);
            stack.push(e);
        }
        assert!(stack.undo(&mut p).is_some());
        assert!(stack.undo(&mut p).is_some());
        assert!(stack.undo(&mut p).is_none());
        assert_eq!(first_tempo(&p), 110.0);
    }

    #[test]
    fn zero_limit_still_keeps_one_edit() {
        let mut p = project();
        let mut stack = UndoStack::with_limit(0);
        let e = tempo_edit(&mut p, 100.0);
        stack.push(e);
        assert!(stack.can_undo());
    }

    #[test]
    fn clear_forgets_everything() {
        let mut p = project();
        let mut stack = UndoStack::new();
        let e = tempo_edit(&mut p, 100.0);
        stack.push(e);
        stack.undo(&mut p);
        stack.clear();
        assert!(!stack.can_undo() && !stack.can_redo());
    }
}
