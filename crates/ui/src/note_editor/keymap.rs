//! Key bindings of the note editor: one table of defaults, overridable from a JSON file
//! (`{ "delete": ["Delete", "Backspace"], "link": "K", "grace": [] }`: an action id maps to one
//! chord or a list, an empty list unbinds it, unlisted actions keep their defaults).
//! Chords read `[Ctrl+][Shift+][Alt+]Key`; Ctrl is Cmd on macOS.

use std::collections::{BTreeMap, HashMap};
use std::fmt;
use std::path::Path;
use std::str::FromStr;

use egui::{InputState, Key};
use fretscribe_core::write_atomic;
use serde::Deserialize;
use thiserror::Error;

use super::NoteAction;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct KeyChord {
    pub key: Key,
    pub ctrl: bool,
    pub shift: bool,
    pub alt: bool,
}

fn key_name(key: Key) -> String {
    let name = format!("{key:?}");
    match key {
        Key::ArrowUp => "Up".into(),
        Key::ArrowDown => "Down".into(),
        Key::ArrowLeft => "Left".into(),
        Key::ArrowRight => "Right".into(),
        Key::OpenBracket => "[".into(),
        Key::CloseBracket => "]".into(),
        _ => name.strip_prefix("Num").filter(|d| d.len() == 1).map_or(name.clone(), str::to_owned),
    }
}

impl KeyChord {
    pub fn matches(&self, input: &InputState) -> bool {
        let m = input.modifiers;
        input.key_pressed(self.key) && m.command == self.ctrl && m.shift == self.shift && m.alt == self.alt
    }
}

impl fmt::Display for KeyChord {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for (on, name) in [(self.ctrl, "Ctrl+"), (self.shift, "Shift+"), (self.alt, "Alt+")] {
            if on {
                f.write_str(name)?;
            }
        }
        f.write_str(&key_name(self.key))
    }
}

impl FromStr for KeyChord {
    type Err = String;

    fn from_str(text: &str) -> Result<Self, String> {
        let mut chord = KeyChord { key: Key::Escape, ctrl: false, shift: false, alt: false };
        let parts: Vec<&str> = text.split('+').map(str::trim).collect();
        let Some((key, modifiers)) = parts.split_last() else {
            return Err(format!("empty shortcut `{text}`"));
        };
        for m in modifiers {
            match m.to_ascii_lowercase().as_str() {
                "ctrl" | "cmd" => chord.ctrl = true,
                "shift" => chord.shift = true,
                "alt" => chord.alt = true,
                _ => return Err(format!("unknown modifier `{m}` in `{text}`")),
            }
        }
        chord.key = Key::ALL.iter().copied().find(|k| key_name(*k).eq_ignore_ascii_case(key)).ok_or_else(|| format!("unknown key `{key}` in `{text}`"))?;
        if chord.key == Key::Escape {
            return Err("Esc is reserved (cancel / clear selection)".into());
        }
        Ok(chord)
    }
}

use NoteAction::*;

/// Action id (the config file key), action, default chords.
const DEFAULTS: &[(&str, NoteAction, &[&str])] = &[
    ("delete", Delete { bridge: false }, &["Delete", "Backspace"]),
    ("delete_bridge", Delete { bridge: true }, &["Shift+Delete"]),
    ("duplicate", Duplicate, &["Ctrl+D"]),
    ("select_all", SelectAll, &["Ctrl+A"]),
    ("select_next", SelectNext { extend: false }, &["Tab"]),
    ("select_previous", SelectPrevious { extend: false }, &["Shift+Tab"]),
    ("select_next_extend", SelectNext { extend: true }, &["Ctrl+Tab"]),
    ("select_previous_extend", SelectPrevious { extend: true }, &["Ctrl+Shift+Tab"]),
    ("transpose_up", Transpose(1), &["Up"]),
    ("transpose_down", Transpose(-1), &["Down"]),
    ("octave_up", Transpose(12), &["Shift+Up"]),
    ("octave_down", Transpose(-12), &["Shift+Down"]),
    ("nudge_earlier", Nudge { steps: -1 }, &["Left"]),
    ("nudge_later", Nudge { steps: 1 }, &["Right"]),
    ("nudge_beat_earlier", NudgeBeat(-1), &["Ctrl+Left"]),
    ("nudge_beat_later", NudgeBeat(1), &["Ctrl+Right"]),
    ("shorten", Resize { steps: -1 }, &["Shift+Left"]),
    ("lengthen", Resize { steps: 1 }, &["Shift+Right"]),
    ("link", LinkHover, &["L"]),
    ("unlink", UnlinkSelection, &["Shift+L"]),
    ("cycle_link_type", CycleLinkType, &["T"]),
    ("grace", Grace, &["G"]),
    ("legato", ForceLegato, &["H"]),
    ("string_1", SetStringFromTop(1), &["1"]),
    ("string_2", SetStringFromTop(2), &["2"]),
    ("string_3", SetStringFromTop(3), &["3"]),
    ("string_4", SetStringFromTop(4), &["4"]),
    ("string_5", SetStringFromTop(5), &["5"]),
    ("string_6", SetStringFromTop(6), &["6"]),
    ("string_7", SetStringFromTop(7), &["7"]),
    ("string_8", SetStringFromTop(8), &["8"]),
    ("string_9", SetStringFromTop(9), &["9"]),
    ("string_auto", SetString(None), &["0"]),
    ("exact_edit", OpenExactEdit, &["Enter", "F2"]),
    ("snap_previous", SnapPreset { next: false }, &["["]),
    ("snap_next", SnapPreset { next: true }, &["]"]),
];

#[derive(Debug, Error, PartialEq)]
pub enum KeymapError {
    #[error("keymap file: {0}")]
    Invalid(String),
    #[error("keymap file: unknown action `{0}`")]
    UnknownAction(String),
    #[error("keymap file: {0} and {1} share the shortcut {2}")]
    Conflict(&'static str, &'static str, KeyChord),
}

#[derive(Deserialize)]
#[serde(untagged)]
enum OneOrMany {
    One(String),
    Many(Vec<String>),
}

#[derive(Clone, Debug, PartialEq)]
pub struct Keymap {
    rows: Vec<(&'static str, NoteAction, Vec<KeyChord>)>,
}

impl Default for Keymap {
    fn default() -> Self {
        Self::build(&HashMap::new()).unwrap_or(Self { rows: Vec::new() })
    }
}

impl Keymap {
    fn build(overrides: &HashMap<String, Vec<String>>) -> Result<Self, KeymapError> {
        if let Some(unknown) = overrides.keys().find(|id| !DEFAULTS.iter().any(|d| d.0 == id.as_str())) {
            return Err(KeymapError::UnknownAction(unknown.clone()));
        }
        let mut rows = Vec::with_capacity(DEFAULTS.len());
        let mut owner: HashMap<String, &'static str> = HashMap::new();
        for &(id, action, defaults) in DEFAULTS {
            let texts: Vec<String> = overrides.get(id).cloned().unwrap_or_else(|| defaults.iter().map(|s| (*s).to_owned()).collect());
            let chords = texts.iter().map(|t| t.parse::<KeyChord>()).collect::<Result<Vec<_>, _>>().map_err(|e| KeymapError::Invalid(format!("{id}: {e}")))?;
            for chord in &chords {
                if let Some(other) = owner.insert(chord.to_string(), id) {
                    return Err(KeymapError::Conflict(other, id, *chord));
                }
            }
            rows.push((id, action, chords));
        }
        Ok(Self { rows })
    }

    pub fn from_json(text: &str) -> Result<Self, KeymapError> {
        let raw: BTreeMap<String, OneOrMany> = serde_json::from_str(text).map_err(|e| KeymapError::Invalid(e.to_string()))?;
        let overrides = raw
            .into_iter()
            .map(|(id, v)| (id, match v { OneOrMany::One(s) => vec![s], OneOrMany::Many(v) => v }))
            .collect();
        Self::build(&overrides)
    }

    /// A missing file gives the defaults silently; an unreadable or invalid one gives the defaults
    /// and a message for the status bar.
    pub fn load_or_default(path: &Path) -> (Self, Option<String>) {
        match std::fs::read_to_string(path) {
            Ok(text) => Self::from_json(&text).map_or_else(|e| (Self::default(), Some(format!("{e} (default shortcuts used)"))), |k| (k, None)),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => (Self::default(), None),
            Err(e) => (Self::default(), Some(format!("keymap file: {e} (default shortcuts used)"))),
        }
    }

    /// Every binding, in the format `from_json` reads: a template to edit.
    pub fn to_json(&self) -> String {
        let lines: Vec<String> = self
            .rows
            .iter()
            .map(|(id, _, chords)| format!("  \"{id}\": {}", serde_json::to_string(&chords.iter().map(ToString::to_string).collect::<Vec<_>>()).unwrap_or_default()))
            .collect();
        format!("{{\n{}\n}}\n", lines.join(",\n"))
    }

    pub fn save(&self, path: &Path) -> std::io::Result<()> {
        write_atomic(path, self.to_json().as_bytes())
    }

    pub fn action_for(&self, input: &InputState) -> Option<NoteAction> {
        self.rows.iter().find(|(_, _, chords)| chords.iter().any(|c| c.matches(input))).map(|r| r.1)
    }

    /// `LinkSelection` shares the link key (`LinkHover` acts on the selection when there is one).
    pub fn chords(&self, action: NoteAction) -> &[KeyChord] {
        let action = if action == LinkSelection { LinkHover } else { action };
        self.rows.iter().find(|r| r.1 == action).map_or(&[], |r| &r.2)
    }

    /// First chord of the action, for tooltips.
    pub fn shortcut_label(&self, action: NoteAction) -> Option<String> {
        self.chords(action).first().map(ToString::to_string)
    }
}

#[cfg(test)]
mod tests {
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
}
