use super::*;
use crate::edit::{NotePolicy, ProjectEdit};
use crate::model::{TimePos, TrackRole};
use crate::tracks::{next_note_id, shift_track};

pub fn project() -> Project {
    crate::testkit::band(8.0)
}

pub fn add_note(p: &mut Project, midi: u8) -> ProjectEdit {
    p.edit_notes("Add note", |p| {
        let id = next_note_id(p);
        p.notes.push(Note::new(
            id,
            midi,
            TimePos::at(1, 0.0),
            TimePos::at(1, 1.0),
        ));
        Ok(())
    })
    .unwrap()
}

pub fn set_capo(p: &mut Project, capo: u8) -> ProjectEdit {
    p.transact_infallible("Capo", |p| p.capo = capo)
}

#[test]
fn notes_stay_sorted_by_id_through_undo_and_redo() {
    let mut p = project();
    for midi in 60..66 {
        add_note(&mut p, midi);
    }
    let full = p.clone();
    let removed = p.remove_notes(&[2, 4]);
    let added = add_note(&mut p, 70);
    added.undo(&mut p);
    removed.undo(&mut p);
    assert_eq!(p, full);
    removed.redo(&mut p);
    assert!(p.notes.windows(2).all(|w| w[0].id < w[1].id));
}

#[test]
fn live_track_fields_survive_undo_and_redo_but_the_rest_reverts() {
    let mut p = project();
    let note = add_note(&mut p, 60);
    let shift = shift_track(&mut p, 1, 0.5, false).unwrap();
    let rename = p.transact_infallible("Rename", |p| p.tracks[1].name = "Kick".into());
    let reorder = p.transact_infallible("Reorder", |p| p.move_track(3, 0).unwrap());
    p.tracks[0].volume = 0.25;
    p.tracks[1].muted = true;
    p.tracks[2].solo = true;
    p.tracks[0].spectrogram_collapsed = true;
    let live_now = |p: &Project| -> Vec<_> {
        let mut v: Vec<_> = p
            .tracks
            .iter()
            .map(|t| (t.id, t.volume, t.muted, t.solo, t.spectrogram_collapsed))
            .collect();
        v.sort_by_key(|t| t.0);
        v
    };
    let sorted = live_now(&p);

    reorder.undo(&mut p);
    assert_eq!(p.tracks.iter().map(|t| t.id).collect::<Vec<_>>(), [1, 2, 3]);
    rename.undo(&mut p);
    assert_eq!(p.tracks[1].name, "Drums");
    shift.undo(&mut p);
    assert_eq!(p.tracks[0].offset_sec, 0.0);
    note.undo(&mut p);
    assert!(p.notes.is_empty());
    assert_eq!(live_now(&p), sorted);
    note.redo(&mut p);
    shift.redo(&mut p);
    assert_eq!((p.tracks[0].offset_sec, p.notes.len()), (0.5, 1));
    assert_eq!(live_now(&p), sorted);

    let merged = note.clone().merge(shift.clone());
    merged.undo(&mut p);
    assert_eq!((p.tracks[0].offset_sec, p.notes.len()), (0.0, 0));
    assert_eq!(live_now(&p), sorted);
}

#[test]
fn removing_a_track_is_undone_with_the_delta_live_values_and_the_source() {
    let mut p = project();
    p.tracks[0].set_volume(0.7);
    p.tracks[0].muted = true;
    let (edit, _) = p.try_edit("Remove track", |p| p.remove_track(1)).unwrap();
    assert_eq!(p.source_track, 2);
    edit.undo(&mut p);
    assert_eq!(p.source_track, 1);
    assert_eq!(
        (p.tracks[0].id, p.tracks[0].volume, p.tracks[0].muted),
        (1, 0.7, true)
    );
}

#[test]
fn reference_pitch_is_undoable_and_validated() {
    let mut p = project();
    add_note(&mut p, 60);
    let before = p.clone();
    let edit = p.set_reference(432.0).unwrap();
    assert_eq!(
        (p.reference_hz, p.notes.clone()),
        (432.0, before.notes.clone())
    );
    edit.undo(&mut p);
    assert_eq!(p, before);
    edit.redo(&mut p);
    assert_eq!(p.reference_hz, 432.0);
    let current = p.clone();
    assert!(p.set_reference(500.0).is_err() && p.set_reference(f32::NAN).is_err());
    assert_eq!(p, current);
}

#[test]
fn a_project_changed_behind_the_history_is_left_alone() {
    let mut p = project();
    let edit = add_note(&mut p, 60);
    p.notes[0].midi = 61;
    let changed = p.clone();
    assert_eq!(edit.try_undo(&mut p), Err(UndoError::Diverged));
    edit.undo(&mut p);
    assert_eq!(p, changed);

    let mut p = project();
    let edit = add_note(&mut p, 60);
    p.tracks[0].volume = 0.1;
    p.tracks[1].muted = true;
    assert_eq!(edit.try_undo(&mut p), Ok(()));
    let undone = set_capo(&mut p, 3);
    p.capo = 4;
    assert_eq!(undone.try_undo(&mut p), Err(UndoError::Diverged));
    assert_eq!(p.capo, 4);
}

#[test]
fn deltas_are_a_small_fraction_of_a_snapshot() {
    let mut p = project();
    p.notes = (1..=3000)
        .map(|id| Note::new(id, 60, TimePos::at(1, 0.0), TimePos::at(1, 1.0)))
        .collect();
    let snapshot = serde_json::to_string(&p).unwrap().len();
    let total: usize = (0..200u64)
        .map(|k| {
            let at = (1 + k * 10) as usize - 1;
            let edit = p
                .edit_notes("Transpose", |p| {
                    p.notes[at].midi = 61 + (k % 5) as u8;
                    Ok(())
                })
                .unwrap();
            serde_json::to_string(&edit).unwrap().len()
        })
        .sum();
    assert!(total < snapshot / 4, "{total} vs {snapshot}");
}

struct Lcg(u64);

impl Lcg {
    fn next(&mut self, n: usize) -> usize {
        self.0 = self
            .0
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        (self.0 >> 33) as usize % n
    }
}

#[test]
fn random_mixed_edits_round_trip_and_merge() {
    let mut p = project();
    let mut rng = Lcg(7);
    let (mut states, mut edits) = (vec![p.clone()], Vec::new());
    for _ in 0..200 {
        let m = rng.next(p.measures.len());
        let edit = match rng.next(9) {
            0..=2 => {
                let (id, beat, midi) = (
                    p.measures[m].id,
                    rng.next(4) as f64,
                    40 + rng.next(40) as u8,
                );
                p.edit_notes("Add note", |p| {
                    let n = next_note_id(p);
                    p.notes.push(Note::new(
                        n,
                        midi,
                        TimePos::at(id, beat),
                        TimePos::at(id, beat + 0.5),
                    ));
                    Ok(())
                })
                .unwrap()
            }
            3 if !p.notes.is_empty() => {
                let id = p.notes[rng.next(p.notes.len())].id;
                p.remove_notes(&[id])
            }
            4 => p
                .set_tempo_scoped(m, 60.0 + rng.next(100) as f64, crate::measures::Propagation::Following, NotePolicy::FollowGrid)
                .unwrap(),
            5 => p.insert_measure(m, NotePolicy::KeepAudioPosition).unwrap(),
            6 if p.measures.len() > 2 => p
                .delete_measure(m, NotePolicy::KeepAudioPosition, true)
                .unwrap(),
            7 if p.tracks.len() < 5 => p.transact_infallible("Add track", |p| {
                p.add_track(Track::new(0, "T", TrackRole::Other));
            }),
            7 => {
                let id = p.tracks[rng.next(p.tracks.len())].id;
                match p.try_edit("Remove track", |p| p.remove_track(id)) {
                    Ok((edit, _)) => edit,
                    Err(_) => continue,
                }
            }
            8 => p.set_reference(430.0 + rng.next(20) as f32).unwrap(),
            _ => set_capo(&mut p, rng.next(5) as u8),
        };
        edits.push(edit);
        states.push(p.clone());
    }
    assert!(edits.len() > 100);
    for (i, pair) in edits.windows(2).enumerate() {
        let merged = pair[0].clone().merge(pair[1].clone());
        let mut q = states[i].clone();
        merged.try_redo(&mut q).unwrap();
        assert_eq!(q, states[i + 2], "merge {i}");
        merged.try_undo(&mut q).unwrap();
        assert_eq!(q, states[i], "unmerge {i}");
    }
    for edit in edits.iter().rev() {
        edit.try_undo(&mut p).unwrap();
    }
    assert_eq!(p, states[0]);
    for edit in &edits {
        edit.try_redo(&mut p).unwrap();
    }
    assert_eq!(Some(&p), states.last());
}
