use super::super::testkit::*;
use super::*;
use fretscribe_core::LinkType;

fn enabled(p: &Project, ids: &[u64]) -> Vec<(&'static str, bool)> {
    let mut s = Selection::default();
    s.set(ids.to_vec());
    action_buttons(p, &s, &Keymap::default(), false).iter().map(|b| (b.label, b.enabled)).collect()
}

#[test]
fn buttons_follow_what_the_selection_allows() {
    let mut p = project();
    let (a, b, c) = (add(&mut p, 64, 0.0, 1.0), add(&mut p, 66, 1.0, 2.0), add(&mut p, 67, 2.0, 3.0));
    assert!(enabled(&p, &[a]).iter().all(|(l, on)| *on == matches!(*l, "H/P" | "Grace" | "Copy" | "Cut" | "Dead" | "Harm")));
    assert!(enabled(&p, &[a, b]).contains(&("Link", true)));
    p.link_notes(a, b, LinkType::Slide).unwrap();
    p.link_notes(b, c, LinkType::Slide).unwrap();
    let e = enabled(&p, &[a]);
    assert!(e.contains(&("Unlink", true)) && e.contains(&("Type", true)) && e.contains(&("Bridge", false)));
    assert!(enabled(&p, &[b]).contains(&("Bridge", true)));
    assert!(enabled(&p, &[c]).contains(&("Unlink", false)));
    assert!(enabled(&p, &[]).iter().all(|(l, on)| !*on) && enabled(&p, &[a]).contains(&("Grace", true)));
}

#[test]
fn tooltips_show_shortcuts() {
    let mut p = project();
    let a = add(&mut p, 64, 0.0, 1.0);
    let mut s = Selection::default();
    s.set(vec![a]);
    let b = action_buttons(&p, &s, &Keymap::default(), false);
    assert_eq!(b[0].tooltip, "Link the selected notes (L)");
    assert_eq!(b[5].tooltip, "Delete and reconnect the neighbours (Shift+Delete)");
    assert_eq!((b[3].tooltip.ends_with("(H)"), b[4].tooltip.ends_with("(G)")), (true, true));
}

#[test]
fn paste_needs_a_clipboard_copy_and_cut_a_selection() {
    let mut p = project();
    let a = add(&mut p, 64, 0.0, 1.0);
    let mut s = Selection::default();
    let on = |s: &Selection, clip| action_buttons(&p, s, &Keymap::default(), clip).iter().filter(|b| matches!(b.label, "Copy" | "Cut" | "Paste")).map(|b| b.enabled).collect::<Vec<_>>();
    assert_eq!(on(&s, false), [false, false, false]);
    assert_eq!(on(&s, true), [false, false, true]);
    s.set(vec![a]);
    assert_eq!(on(&s, false), [true, true, false]);
    assert_eq!(action_buttons(&p, &s, &Keymap::default(), true)[8].tooltip, "Paste the copied notes (click to place them) (Ctrl+V)");
}
