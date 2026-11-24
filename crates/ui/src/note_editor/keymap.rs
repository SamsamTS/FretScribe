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
    ("copy", Copy, &["Ctrl+C"]),
    ("cut", Cut, &["Ctrl+X"]),
    ("paste", Paste, &["Ctrl+V"]),
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
    ("cycle_link_type", CycleLinkType, &[]),
    ("grace", Grace, &["G"]),
    ("legato", ForceLegato, &["H"]),
    ("dead_note", ToggleDead, &["X"]),
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

    /// `LinkSelection` shares the link key (`LinkHover` acts on the selection when there is one),
    /// and so does `CycleLinkType` unless it is given a key of its own.
    pub fn chords(&self, action: NoteAction) -> &[KeyChord] {
        let find = |a: NoteAction| self.rows.iter().find(|r| r.1 == a).map_or(&[][..], |r| &r.2[..]);
        match action {
            LinkSelection => find(LinkHover),
            CycleLinkType if find(CycleLinkType).is_empty() => find(LinkHover),
            _ => find(action),
        }
    }

    /// First chord of the action, for tooltips.
    pub fn shortcut_label(&self, action: NoteAction) -> Option<String> {
        self.chords(action).first().map(ToString::to_string)
    }
}

#[cfg(all(test, dev_tests))]
#[path = "../../../../dev/tests/unit/ui/note_editor/keymap.rs"]
mod tests;
