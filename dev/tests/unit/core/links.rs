use super::*;
use crate::testkit::add;
use crate::model::{GraceKind, GraceNote, TimePos};


fn project() -> Project {
    crate::testkit::project(8.0)
}

fn chain3(p: &mut Project) -> [u64; 3] {
    let ids = [
        add(p, 60, 0.0, 1.0),
        add(p, 62, 1.0, 2.0),
        add(p, 64, 2.0, 3.0),
    ];
    p.link_notes(ids[0], ids[1], LinkType::Slide).unwrap();
    p.link_notes(ids[1], ids[2], LinkType::Slide).unwrap();
    ids
}

#[test]
fn link_stores_slide_and_bend_on_the_start_note_and_legato_on_the_end_note() {
    let mut p = project();
    let (a, b, c, d) = (
        add(&mut p, 60, 0.0, 1.0),
        add(&mut p, 62, 1.0, 2.0),
        add(&mut p, 64, 2.0, 3.0),
        add(&mut p, 62, 3.0, 4.0),
    );
    p.link_notes(a, b, LinkType::Bend).unwrap();
    assert_eq!(
        (p.note(a).unwrap().link_to, p.note(a).unwrap().technique),
        (Some(b), Some(Technique::Bend))
    );
    p.link_notes(b, c, LinkType::Legato).unwrap();
    assert_eq!(
        p.note(c).unwrap().legato,
        Some(LegatoLink {
            kind: Legato::HammerOn,
            from: b
        })
    );
    assert_eq!(p.note(b).unwrap().link_to, None);
    p.link_notes(c, d, LinkType::Legato).unwrap();
    assert_eq!(
        p.note(d).unwrap().legato.map(|l| l.kind),
        Some(Legato::PullOff)
    );
    assert_eq!(p.outgoing(b), Some((c, LinkType::Legato)));
    assert_eq!(p.incoming(b), Some((a, LinkType::Bend)));
    assert_eq!(p.chain_of(c), [a, b, c, d]);
}

#[test]
fn invariants_are_enforced() {
    let mut p = project();
    let [a, b, c] = chain3(&mut p);
    let d = add(&mut p, 67, 3.0, 4.0);
    assert_eq!(
        p.link_notes(a, d, LinkType::Slide),
        Err(LinkError::HasOutgoing(a))
    );
    assert_eq!(
        p.link_notes(c, b, LinkType::Slide),
        Err(LinkError::NotInOrder)
    );
    assert_eq!(
        p.link_notes(d, b, LinkType::Slide),
        Err(LinkError::NotInOrder)
    );
    let e = add(&mut p, 69, 2.5, 3.5);
    assert_eq!(
        p.link_notes(e, c, LinkType::Slide),
        Err(LinkError::NotInOrder)
    );
    assert_eq!(
        p.link_notes(b, e, LinkType::Slide),
        Err(LinkError::HasOutgoing(b))
    );
    let f = add(&mut p, 70, 5.0, 6.0);
    assert_eq!(
        p.link_notes(f, c, LinkType::Slide),
        Err(LinkError::NotInOrder)
    );
    assert_eq!(
        p.link_notes(c, c, LinkType::Slide),
        Err(LinkError::SameNote)
    );
    assert_eq!(
        p.link_notes(c, 99, LinkType::Slide),
        Err(LinkError::UnknownNote(99))
    );
    assert_eq!(p.link_notes(c, d, LinkType::Slide), Ok(()));
    assert_eq!(
        p.link_notes(e, d, LinkType::Slide),
        Err(LinkError::HasIncoming(d))
    );
    let same = add(&mut p, 64, 6.0, 7.0);
    assert_eq!(p.link_notes(f, same, LinkType::Slide), Ok(()));
    let twin = add(&mut p, 70, 7.0, 8.0);
    assert_eq!(p.link_notes(same, twin, LinkType::Slide), Ok(()));
    let flat = add(&mut p, 70, 7.5, 8.0);
    assert_eq!(
        p.link_notes(twin, flat, LinkType::Slide),
        Err(LinkError::SamePitch)
    );
}

#[test]
fn cycles_are_rejected_between_on_the_beat_grace_notes() {
    let mut p = project();
    let main = add(&mut p, 64, 0.0, 2.0);
    let (a, b) = (add(&mut p, 60, 0.0, 0.5), add(&mut p, 62, 0.0, 0.5));
    for n in p.notes.iter_mut().filter(|n| n.id != main) {
        n.grace_of = Some(GraceNote {
            main,
            kind: GraceKind::Appoggiatura,
        });
    }
    p.link_notes(a, b, LinkType::Slide).unwrap();
    assert_eq!(
        p.note(a).unwrap().end,
        TimePos::at(1, 0.5),
        "on-the-beat grace notes keep their length"
    );
    assert_eq!(p.link_notes(b, a, LinkType::Slide), Err(LinkError::Cycle));
}

#[test]
fn linking_ties_the_end_of_the_source_to_the_start_of_the_destination() {
    let mut p = project();
    let (a, b, c) = (
        add(&mut p, 60, 0.0, 3.0),
        add(&mut p, 62, 1.0, 2.0),
        add(&mut p, 64, 5.0, 6.0),
    );
    p.link_notes(a, b, LinkType::Slide).unwrap();
    assert_eq!(
        p.note(a).unwrap().end,
        TimePos::at(1, 1.0),
        "a long source is cut"
    );
    p.link_notes(b, c, LinkType::Slide).unwrap();
    assert_eq!(
        p.note(b).unwrap().end,
        TimePos::at(1, 5.0),
        "a short source is extended"
    );
    let d = add(&mut p, 65, 5.0, 6.0);
    assert_eq!(
        p.link_notes(c, d, LinkType::Slide),
        Err(LinkError::NotInOrder),
        "the source would vanish"
    );
}

#[test]
fn tie_links_follows_the_end_that_was_not_touched_and_drops_links_that_cannot_hold() {
    let mut p = project();
    let [a, b, c] = chain3(&mut p);
    p.notes.iter_mut().find(|n| n.id == b).unwrap().start = TimePos::at(1, 1.5);
    assert_eq!(p.tie_links(&[b]), [a]);
    assert_eq!(p.note(a).unwrap().end, TimePos::at(1, 1.5));
    p.notes.iter_mut().find(|n| n.id == b).unwrap().end = TimePos::at(1, 2.5);
    assert_eq!(p.tie_links(&[b]), [c]);
    assert_eq!(p.note(c).unwrap().start, TimePos::at(1, 2.5));
    p.notes.iter_mut().find(|n| n.id == c).unwrap().end = TimePos::at(1, 2.51);
    p.notes.iter_mut().find(|n| n.id == b).unwrap().end = TimePos::at(1, 2.6);
    assert!(p.tie_links(&[b]).is_empty());
    assert_eq!(p.outgoing(b), None, "c would be left under a lattice unit");
}

#[test]
fn type_rules() {
    let mut p = project();
    let a = add(&mut p, 60, 0.0, 1.0);
    let far = add(&mut p, 66, 1.0, 2.0);
    assert_eq!(
        p.link_notes(a, far, LinkType::Bend),
        Err(LinkError::BendInterval)
    );
    let low = add(&mut p, 40, 3.0, 3.5);
    let high = add(&mut p, 65, 3.5, 4.0);
    assert_eq!(
        p.link_notes(low, high, LinkType::Legato),
        Err(LinkError::NoSameString)
    );
    assert_eq!(p.infer_link_type(low, high), LinkType::Slide);
    assert_eq!(p.infer_link_type(a, far), LinkType::Legato);
    let distant = add(&mut p, 62, 3.9, 4.0);
    assert_eq!(
        p.infer_link_type(a, distant),
        LinkType::Slide,
        "legato needs adjacent notes"
    );
    let gap = add(&mut p, 62, 3.9, 4.0);
    assert_eq!(
        p.link_notes(a, gap, LinkType::Legato),
        Err(LinkError::NotAdjacent)
    );
    p.link_notes(low, high, LinkType::Slide).unwrap();
    assert!(!p.chain_fits_one_string(low));
    assert!(p.chain_fits_one_string(a));
}

#[test]
fn unlink_and_split_chain() {
    let mut p = project();
    let [a, b, c] = chain3(&mut p);
    assert!(p.unlink(a));
    assert!(!p.unlink(a));
    assert_eq!(p.chain_of(b), [b, c]);
    assert!(p.note(a).unwrap().technique.is_none());
    p.link_notes(a, b, LinkType::Legato).unwrap();
    assert!(p.split_chain(c));
    assert_eq!(p.chain_of(a), [a, b]);
    assert!(!p.split_chain(a));
    assert!(p.unlink(a));
    assert_eq!(p.note(b).unwrap().legato, None);
}

#[test]
fn cycle_walks_slide_bend_legato_and_skips_invalid_states() {
    let mut p = project();
    let (a, b) = (add(&mut p, 60, 0.0, 1.0), add(&mut p, 62, 1.0, 2.0));
    p.link_notes(a, b, LinkType::Slide).unwrap();
    assert_eq!(p.cycle_link_type(a), Some(LinkType::Bend));
    assert_eq!(p.cycle_link_type(a), Some(LinkType::Legato));
    assert_eq!(p.cycle_link_type(a), Some(LinkType::Slide));
    assert_eq!(p.outgoing(a), Some((b, LinkType::Slide)));

    let c = add(&mut p, 70, 2.0, 3.0);
    p.link_notes(b, c, LinkType::Slide).unwrap();
    assert_eq!(p.cycle_link_type(b), Some(LinkType::Legato));
    assert_eq!(p.cycle_link_type(b), Some(LinkType::Slide));

    let d = add(&mut p, 40, 4.0, 5.0);
    let e = add(&mut p, 65, 5.0, 6.0);
    p.link_notes(d, e, LinkType::Slide).unwrap();
    assert_eq!(p.cycle_link_type(d), None);
    assert_eq!(p.outgoing(d), Some((e, LinkType::Slide)));
    assert_eq!(p.cycle_link_type(e), None);
}

#[test]
fn deleting_the_ends_and_the_middle_of_a_chain() {
    let mut p = project();
    let [a, b, c] = chain3(&mut p);
    p.delete_notes(&[a], false);
    assert_eq!(p.chain_of(b), [b, c]);
    let mut p = project();
    let [a, b, c] = chain3(&mut p);
    p.delete_notes(&[c], false);
    assert_eq!(p.chain_of(a), [a, b]);
    assert_eq!(p.note(b).unwrap().link_to, None);
    assert_eq!(p.note(b).unwrap().technique, None);
    let mut p = project();
    let [a, b, c] = chain3(&mut p);
    p.delete_notes(&[b], false);
    assert_eq!((p.chain_of(a), p.chain_of(c)), (vec![a], vec![c]));
}

#[test]
fn bridging_reconnects_the_neighbours_and_undoes_in_one_step() {
    let mut p = project();
    let [a, b, c] = chain3(&mut p);
    let before = p.notes.clone();
    let edit = p.delete_notes(&[b], true);
    assert_eq!(p.chain_of(a), [a, c]);
    assert_eq!(p.outgoing(a), Some((c, LinkType::Slide)));
    assert_eq!(
        p.note(a).unwrap().end,
        p.note(c).unwrap().start,
        "the bridged note is tied to its new destination"
    );
    edit.undo(&mut p);
    assert_eq!(p.notes, before);
    edit.redo(&mut p);
    assert_eq!(p.chain_of(a), [a, c]);
}

#[test]
fn bridging_over_several_deleted_notes() {
    let mut p = project();
    let ids = [
        add(&mut p, 60, 0.0, 1.0),
        add(&mut p, 62, 1.0, 2.0),
        add(&mut p, 64, 2.0, 3.0),
        add(&mut p, 67, 3.0, 4.0),
    ];
    for w in ids.windows(2) {
        p.link_notes(w[0], w[1], LinkType::Slide).unwrap();
    }
    p.delete_notes(&[ids[1], ids[2]], true);
    assert_eq!(p.chain_of(ids[0]), [ids[0], ids[3]]);
}

#[test]
fn link_selection_chains_single_notes_in_time_order() {
    let mut p = project();
    let ids = [
        add(&mut p, 62, 1.0, 2.0),
        add(&mut p, 60, 0.0, 1.0),
        add(&mut p, 64, 2.0, 3.0),
    ];
    let report = p.link_selection(&ids);
    assert_eq!((report.linked, report.skipped.len()), (2, 0));
    assert_eq!(p.chain_of(ids[0]), [ids[1], ids[0], ids[2]]);
    assert_eq!(p.outgoing(ids[1]), Some((ids[0], LinkType::Legato)));
}

#[test]
fn link_selection_pairs_chords_by_pitch_order() {
    let mut p = project();
    let low = [add(&mut p, 57, 0.0, 1.0), add(&mut p, 64, 0.0, 1.0)];
    let high = [add(&mut p, 66, 1.0, 2.0), add(&mut p, 59, 1.0, 2.0)];
    let all = [low[1], low[0], high[0], high[1]];
    let report = p.link_selection(&all);
    assert_eq!(report.linked, 2);
    assert_eq!(p.chain_of(low[0]), [low[0], high[1]]);
    assert_eq!(p.chain_of(low[1]), [low[1], high[0]]);
}

#[test]
fn link_selection_with_unequal_groups_links_only_existing_pairs() {
    let mut p = project();
    let a = [
        add(&mut p, 57, 0.0, 1.0),
        add(&mut p, 64, 0.0, 1.0),
        add(&mut p, 69, 0.0, 1.0),
    ];
    let b = [add(&mut p, 59, 1.0, 2.0), add(&mut p, 66, 1.0, 2.0)];
    let ids: Vec<u64> = a.iter().chain(&b).copied().collect();
    let report = p.link_selection(&ids);
    assert_eq!(report.linked, 2);
    assert_eq!(p.outgoing(a[2]), None);
}

#[test]
fn overlapping_notes_are_never_linked_to_each_other() {
    let mut p = project();
    let ids = [add(&mut p, 60, 0.0, 2.0), add(&mut p, 62, 1.0, 3.0)];
    let report = p.link_selection(&ids);
    assert_eq!((report.linked, report.skipped.len()), (0, 0));
    assert!(p.outgoing(ids[0]).is_none());
}

#[test]
fn same_pitch_neighbours_are_tied_and_a_slide_can_follow() {
    let mut p = project();
    let ids = [add(&mut p, 60, 0.0, 1.0), add(&mut p, 60, 1.0, 2.0), add(&mut p, 64, 2.0, 3.0)];
    let report = p.link_selection(&ids);
    assert_eq!((report.linked, report.skipped.len()), (2, 0));
    assert_eq!(p.outgoing(ids[0]), Some((ids[1], LinkType::Tied)));
    assert_eq!(p.note(ids[0]).unwrap().technique, Some(Technique::Tie));
    assert!(matches!(p.outgoing(ids[1]), Some((_, LinkType::Legato | LinkType::Slide))));
    assert_eq!(p.chain_of(ids[0]), ids);
    assert_eq!(p.can_link(ids[0], ids[2], LinkType::Tied), Err(LinkError::HasOutgoing(ids[0])));
    let mut q = project();
    let (a, b) = (add(&mut q, 60, 0.0, 1.0), add(&mut q, 62, 1.0, 2.0));
    assert_eq!(q.can_link(a, b, LinkType::Tied), Err(LinkError::TiedNeedsSamePitch));
    let c = add(&mut q, 62, 5.0, 6.0);
    assert_eq!(q.can_link(b, c, LinkType::Tied), Err(LinkError::NotAdjacent));
    assert!(p.unlink(ids[0]) && p.note(ids[0]).unwrap().technique.is_none());
}

#[test]
fn link_selection_is_one_undoable_edit() {
    let mut p = project();
    let ids = [
        add(&mut p, 60, 0.0, 1.0),
        add(&mut p, 62, 1.0, 2.0),
        add(&mut p, 64, 2.0, 3.0),
    ];
    let before = p.notes.clone();
    let edit = p.edit_notes("Link", |p| {
        p.link_selection(&ids);
        Ok(())
    });
    let edit = edit.unwrap();
    assert_ne!(p.notes, before);
    edit.undo(&mut p);
    assert_eq!(p.notes, before);
}

#[test]
fn dead_and_harmonic_notes_take_no_link() {
    for technique in [Technique::Dead, Technique::Harmonic, Technique::PinchHarmonic] {
        let mut p = project();
        let (a, b, c) = (add(&mut p, 60, 0.0, 1.0), add(&mut p, 62, 1.0, 2.0), add(&mut p, 64, 2.0, 3.0));
        p.notes[1].technique = Some(technique);
        for kind in [LinkType::Slide, LinkType::Bend, LinkType::Legato] {
            assert_eq!(p.link_notes(a, b, kind), Err(LinkError::Ineligible(b)));
            assert_eq!(p.link_notes(b, c, kind), Err(LinkError::Ineligible(b)));
        }
        assert_eq!(p.link_selection(&[a, b, c]).linked, 0);
        assert!(p.force_legato(a, b).is_err());
    }
}
