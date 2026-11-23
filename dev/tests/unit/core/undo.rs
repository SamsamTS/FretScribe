use super::*;
use crate::delta::tests::{add_note, project, set_capo};
use crate::edit::NotePolicy;
use crate::persist::ProjectFile;

fn tempo_edit(p: &mut Project, bpm: f64) -> ProjectEdit {
    p.set_tempo_scoped(0, bpm, crate::measures::Propagation::Following, NotePolicy::FollowGrid).unwrap()
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

    assert_eq!(
        stack.undo(&mut p).map(ProjectEdit::label),
        Some("Set tempo")
    );
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

fn capo_run(p: &mut Project, stack: &mut UndoStack, n: usize) {
    for _ in 0..n {
        let next = (p.capo + 1) % 12;
        stack.push(crate::delta::tests::set_capo(p, next));
    }
}

fn weights(stack: &UndoStack) -> Vec<usize> {
    stack.done.iter().map(ProjectEdit::weight).collect()
}

#[test]
fn thinning_protects_the_newest_entries_and_keeps_levels_small() {
    let mut p = project();
    let mut stack = UndoStack::new();
    capo_run(&mut p, &mut stack, 15);
    assert_eq!(weights(&stack), vec![1; 15]);
    capo_run(&mut p, &mut stack, 85);
    let w = weights(&stack);
    assert!(w[w.len() - 20..].iter().all(|&x| x == 1));
    assert!(w.windows(2).all(|pair| pair[0] >= pair[1]));
    let old = &w[..w.len() - 20];
    assert!(old
        .iter()
        .all(|x| old.iter().filter(|y| y == &x).count() <= 3));
    assert_eq!(w.iter().sum::<usize>(), 100);
}

#[test]
fn history_stays_logarithmic_and_walks_back_to_the_start() {
    let mut p = project();
    let (original, mut stack) = (p.clone(), UndoStack::new());
    capo_run(&mut p, &mut stack, 1000);
    assert!(
        stack.done.len() < (20.0 + 3.0 * (1000f64.log2() + 2.0)) as usize,
        "{}",
        stack.done.len()
    );
    assert_eq!(weights(&stack).iter().sum::<usize>(), 1000);
    let (end, mut steps) = (p.clone(), 0);
    while stack.undo(&mut p).is_some() {
        steps += 1;
    }
    assert_eq!(p, original);
    for _ in 0..steps {
        stack.redo(&mut p).unwrap();
    }
    assert_eq!(p, end);
}

#[test]
fn weights_are_reported_and_kept_by_push_merged() {
    let mut p = project();
    let mut stack = UndoStack::new();
    capo_run(&mut p, &mut stack, 40);
    assert_eq!((stack.undo_weight(), stack.redo_weight()), (1, 0));
    let e = crate::delta::tests::set_capo(&mut p, 11);
    stack.push_merged(e);
    assert_eq!(
        (stack.undo_weight(), stack.done.len()),
        (1, weights(&stack).len())
    );
    let heaviest = (0..stack.done.len())
        .map(|_| stack.undo(&mut p).map(ProjectEdit::weight).unwrap())
        .last()
        .unwrap();
    assert!(heaviest > 1);
    assert_eq!(stack.redo_weight(), heaviest);
    assert!(stack.undo(&mut p).is_none() && stack.undo_weight() == 0);
}

#[test]
fn policy_is_sanitised_and_applies_at_the_next_push() {
    let bad = UndoPolicy {
        min_entries: 5,
        per_level: 0,
        max_entries: 3,
    };
    assert_eq!(
        bad.sanitised(),
        UndoPolicy {
            min_entries: 2,
            per_level: 1,
            max_entries: 3
        }
    );
    assert_eq!(
        UndoPolicy {
            max_entries: 0,
            ..bad
        }
        .sanitised()
        .max_entries,
        1
    );
    let mut p = project();
    let mut stack = UndoStack::new();
    capo_run(&mut p, &mut stack, 30);
    let len = stack.done.len();
    stack.set_policy(UndoPolicy {
        min_entries: 2,
        per_level: 1,
        max_entries: 6,
    });
    assert_eq!(stack.done.len(), len);
    capo_run(&mut p, &mut stack, 1);
    assert!(stack.done.len() <= 6);
    let mut capped = UndoStack::with_policy(UndoPolicy {
        min_entries: 50,
        per_level: 100,
        max_entries: 60,
    });
    capo_run(&mut p, &mut capped, 80);
    assert_eq!(
        (capped.done.len(), weights(&capped).iter().sum::<usize>()),
        (60, 60)
    );
}

#[test]
fn divergence_clears_the_history_once() {
    let mut p = project();
    let mut stack = UndoStack::new();
    stack.push(add_note(&mut p, 60));
    stack.push(set_capo(&mut p, 2));
    p.tracks[0].volume = 0.3;
    stack.undo(&mut p).unwrap();
    assert!(!stack.take_diverged());
    p.notes[0].midi = 72;
    assert!(stack.undo(&mut p).is_none());
    assert!(stack.take_diverged() && !stack.take_diverged());
    assert!(!stack.can_undo() && !stack.can_redo());
    assert_eq!(p.notes[0].midi, 72);
}

fn history_file(stack: &UndoStack, p: &Project) -> ProjectFile {
    let mut file = ProjectFile::new(p.clone());
    file.history = stack.to_saved();
    file
}

fn mixed_history() -> (Project, Project, UndoStack) {
    let mut p = project();
    let original = p.clone();
    let mut stack = UndoStack::new();
    capo_run(&mut p, &mut stack, 30);
    stack.push(add_note(&mut p, 60));
    stack.push(p.set_tempo_scoped(1, 90.0, crate::measures::Propagation::Following, NotePolicy::FollowGrid).unwrap());
    stack.push(crate::tracks::shift_track(&mut p, 2, 0.25, false).unwrap());
    stack.push(p.remove_notes(&[1]));
    (original, p, stack)
}

#[test]
fn saved_history_replays_step_by_step_after_loading() {
    let (original, p, mut stack) = mixed_history();
    assert!(stack.done.iter().any(|e| e.weight() > 1));
    let mut loaded =
        crate::persist::from_json(&crate::persist::to_json(&history_file(&stack, &p)).unwrap())
            .unwrap();
    let mut restored = UndoStack::from_saved(
        loaded.history.take().unwrap(),
        &loaded.project,
        UndoPolicy::default(),
    );
    assert!(!restored.can_redo() && restored.done.len() == stack.done.len());
    let (mut a, mut b) = (p.clone(), loaded.project);
    while stack.undo(&mut a).is_some() {
        assert!(restored.undo(&mut b).is_some());
        assert_eq!(a, b);
    }
    assert_eq!(b, original);
    assert!(restored.undo(&mut b).is_none());
}

#[test]
fn broken_or_stale_history_keeps_the_newest_valid_run() {
    let mut p = project();
    let mut stack = UndoStack::new();
    stack.push(add_note(&mut p, 60));
    capo_run(&mut p, &mut stack, 5);
    let saved = stack.to_saved().unwrap();
    let mut json = serde_json::to_value(&saved).unwrap();
    json["done"][2]["delta"]["capo"]["after"] = serde_json::json!(99);
    let broken = UndoStack::from_saved(
        serde_json::from_value(json).unwrap(),
        &p,
        UndoPolicy::default(),
    );
    assert_eq!(broken.done.len(), 3);

    p.notes[0].midi = 80;
    let stale = UndoStack::from_saved(saved, &p, UndoPolicy::default());
    assert_eq!(stale.done.len(), 5);
    p.capo = 0;
    assert!(
        UndoStack::from_saved(stack.to_saved().unwrap(), &p, UndoPolicy::default())
            .done
            .is_empty()
    );
    assert!(UndoStack::new().to_saved().is_none());
}

#[test]
fn loading_applies_the_given_policy() {
    let mut p = project();
    let mut stack = UndoStack::with_limit(1000);
    capo_run(&mut p, &mut stack, 30);
    let policy = UndoPolicy {
        min_entries: 2,
        per_level: 1,
        max_entries: 5,
    };
    let loaded = UndoStack::from_saved(stack.to_saved().unwrap(), &p, policy);
    assert!(loaded.done.len() <= 5);
}
