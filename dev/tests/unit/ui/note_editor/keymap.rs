use super::*;
use egui::{Context, Event, Modifiers, RawInput};

fn press(map: &Keymap, chord: &KeyChord) -> Option<NoteAction> {
    let modifiers = Modifiers { ctrl: chord.ctrl, command: chord.ctrl, shift: chord.shift, alt: chord.alt, ..Modifiers::NONE };
    let event = Event::Key { key: chord.key, physical_key: Some(chord.key), pressed: true, repeat: false, modifiers };
    let mut out = None;
    let _ = Context::default().run(RawInput { events: vec![event], modifiers, ..RawInput::default() }, |ctx| out = ctx.input(|i| map.action_for(i)));
    out
}

#[test]
fn defaults_are_valid_and_every_binding_dispatches_its_action() {
    let map = Keymap::default();
    assert_eq!(map.rows.len(), DEFAULTS.len());
    for (id, action, chords) in &map.rows {
        assert!(!chords.is_empty(), "{id} has no default shortcut");
        for chord in chords {
            assert_eq!(press(&map, chord), Some(*action), "{id} {chord}");
        }
    }
}

#[test]
fn chords_round_trip_and_reject_garbage() {
    for text in ["Ctrl+Shift+Tab", "Shift+Delete", "[", "0", "F2", "Alt+Up", "A"] {
        assert_eq!(text.parse::<KeyChord>().unwrap().to_string(), text);
    }
    assert_eq!("ctrl + d".parse::<KeyChord>().unwrap().to_string(), "Ctrl+D");
    for bad in ["", "Ctrl+", "Hyper+A", "Banana", "Esc"] {
        assert!(bad.parse::<KeyChord>().is_err(), "{bad}");
    }
}

#[test]
fn exact_modifiers_are_required() {
    let map = Keymap::default();
    let up = |ctrl, shift| KeyChord { key: Key::ArrowUp, ctrl, shift, alt: false };
    assert_eq!(press(&map, &up(false, false)), Some(Transpose(1)));
    assert_eq!(press(&map, &up(false, true)), Some(Transpose(12)));
    assert_eq!(press(&map, &up(true, false)), None);
}

#[test]
fn config_overrides_unbinds_and_keeps_the_rest() {
    let map = Keymap::from_json(r#"{ "link": "K", "grace": [], "delete": ["X", "Backspace"] }"#).unwrap();
    assert_eq!(press(&map, &"K".parse().unwrap()), Some(LinkHover));
    assert_eq!(press(&map, &"L".parse().unwrap()), None);
    assert_eq!(press(&map, &"G".parse().unwrap()), None);
    assert_eq!(press(&map, &"X".parse().unwrap()), Some(Delete { bridge: false }));
    assert_eq!(press(&map, &"H".parse().unwrap()), Some(ForceLegato));
    assert_eq!(map.shortcut_label(LinkHover).as_deref(), Some("K"));
    assert_eq!(map.shortcut_label(Grace), None);
}

#[test]
fn bad_configs_are_reported() {
    assert!(matches!(Keymap::from_json(r#"{ "lnk": "K" }"#), Err(KeymapError::UnknownAction(_))));
    assert!(matches!(Keymap::from_json(r#"{ "link": "Banana" }"#), Err(KeymapError::Invalid(_))));
    assert!(matches!(Keymap::from_json("not json"), Err(KeymapError::Invalid(_))));
    assert!(matches!(Keymap::from_json(r#"{ "link": "G" }"#), Err(KeymapError::Conflict("link", "grace", _)) | Err(KeymapError::Conflict("grace", "link", _))));
    assert!(Keymap::from_json(r#"{ "link": "G", "grace": "K" }"#).is_ok());
}

#[test]
fn json_template_round_trips_and_the_file_loader_falls_back() {
    let map = Keymap::default();
    assert_eq!(Keymap::from_json(&map.to_json()).unwrap(), map);
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("keymap.json");
    assert_eq!(Keymap::load_or_default(&path), (map.clone(), None));
    std::fs::write(&path, r#"{ "link": "K" }"#).unwrap();
    assert_eq!(Keymap::load_or_default(&path).0.shortcut_label(LinkHover).as_deref(), Some("K"));
    std::fs::write(&path, "{ oops").unwrap();
    let (fallback, message) = Keymap::load_or_default(&path);
    assert_eq!(fallback, map);
    assert!(message.is_some_and(|m| m.contains("default shortcuts")));
    map.save(&path).unwrap();
    assert_eq!(Keymap::load_or_default(&path), (map, None));
}
