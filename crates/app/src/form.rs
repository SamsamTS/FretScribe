//! New-project form logic: parsing, validation and project construction (no UI).

use std::path::{Path, PathBuf};

use fretscribe_core::{PitchRef, Project, TimeSignature, Track, TrackRole, Tuning, MAX_BPM, MIN_BPM};
use fretscribe_notation::tab::presets;

pub const MAX_CAPO: u8 = 12;
const NAMES: [&str; 12] = ["C", "C#", "D", "D#", "E", "F", "F#", "G", "G#", "A", "A#", "B"];

/// Only selects the parse defaults and presets; `Project` has no instrument field.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Instrument {
    #[default]
    Guitar,
    Bass,
}

impl Instrument {
    fn bottom_string(self) -> i32 {
        match self {
            Self::Guitar => 40,
            Self::Bass => 28,
        }
    }
}

fn parse_note(token: &str) -> Result<(i32, Option<i32>), String> {
    let bad = || format!("'{token}' is not a note (use e.g. E, F#, Bb, E2)");
    let mut chars = token.chars();
    let letter = chars.next().ok_or_else(bad)?.to_ascii_uppercase();
    let base: i32 = match letter {
        'C' => 0,
        'D' => 2,
        'E' => 4,
        'F' => 5,
        'G' => 7,
        'A' => 9,
        'B' => 11,
        _ => return Err(bad()),
    };
    let rest = chars.as_str();
    let (shift, digits) = match rest.chars().next() {
        Some('#') => (1, &rest[1..]),
        Some('b') => (-1, &rest[1..]),
        _ => (0, rest),
    };
    let pitch_class = (base + shift).rem_euclid(12);
    let octave = match digits {
        "" => None,
        d if d.len() <= 2 && d.bytes().all(|b| b.is_ascii_digit()) => Some(12 * (d.parse::<i32>().map_err(|_| bad())? + 1) + base + shift),
        _ => return Err(bad()),
    };
    Ok((pitch_class, octave))
}

/// Splits "EADGBe" or "E A D G B e" into note tokens: a lowercase `b` is a flat only inside a
/// separated token ("Bb"), a bare run of letters treats it as the note B.
fn tokenize(text: &str) -> Vec<String> {
    let mut tokens = Vec::new();
    for chunk in text.split(|c: char| c.is_whitespace() || c == ',').filter(|t| !t.is_empty()) {
        if parse_note(chunk).is_ok() {
            tokens.push(chunk.to_owned());
            continue;
        }
        let mut current = String::new();
        for c in chunk.chars() {
            let continues = c == '#' || c.is_ascii_digit();
            if !continues && !current.is_empty() {
                tokens.push(std::mem::take(&mut current));
            }
            current.push(c);
        }
        tokens.extend((!current.is_empty()).then_some(current));
    }
    tokens
}

/// Note names lowest string first (DADGAD); open strings come back in the same order.
pub fn parse_tuning(text: &str, instrument: Instrument) -> Result<Tuning, String> {
    let tokens = tokenize(text);
    if !(4..=8).contains(&tokens.len()) {
        return Err("Enter 4 to 8 notes, lowest string first (e.g. E A D G B E)".to_owned());
    }
    let mut midis: Vec<i32> = Vec::new();
    for token in &tokens {
        let (class, octave) = parse_note(token)?;
        let midi = match (octave, midis.last()) {
            (Some(m), _) => m,
            (None, Some(&prev)) => prev + 1 + (class - prev - 1).rem_euclid(12),
            (None, None) => {
                let bottom = instrument.bottom_string();
                bottom + (class - bottom + 5).rem_euclid(12) - 5
            }
        };
        if !(0..=127).contains(&midi) {
            return Err(format!("'{token}' is out of range"));
        }
        if midis.last().is_some_and(|&prev| midi < prev || midi > prev + 24) {
            return Err(format!("'{token}' must be higher than the string before it"));
        }
        midis.push(midi);
    }
    let open_strings: Vec<u8> = midis.iter().map(|&m| m as u8).collect();
    let name = presets()
        .into_iter()
        .find(|p| p.open_strings == open_strings)
        .map_or_else(|| "Custom".to_owned(), |p| p.name);
    Ok(Tuning { name, open_strings })
}

pub fn tuning_text(tuning: &Tuning) -> String {
    tuning
        .open_strings
        .iter()
        .enumerate()
        .map(|(i, &m)| {
            let name = NAMES[usize::from(m % 12)];
            if i + 1 == tuning.open_strings.len() { name.to_lowercase() } else { name.to_owned() }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

pub fn preset_texts(instrument: Instrument) -> Vec<(&'static str, String)> {
    let all = presets();
    let text = |name: &str| all.iter().find(|t| t.name == name).map(tuning_text).unwrap_or_default();
    match instrument {
        Instrument::Guitar => [
            ("Standard", "Standard"),
            ("Drop D", "Drop D"),
            ("DADGAD", "DADGAD"),
            ("Half step down", "Half-step down"),
            ("Open G", "Open G"),
            ("Open D", "Open D"),
        ]
        .map(|(label, name)| (label, text(name)))
        .to_vec(),
        Instrument::Bass => vec![("Standard", text("Bass 4-string")), ("Drop D", "D A D g".to_owned())],
    }
}

/// `Title - Album by Artist` with missing parts dropped; the flag says the tags were used.
pub fn project_name(title: Option<&str>, album: Option<&str>, artist: Option<&str>, file_stem: &str) -> (String, bool) {
    fn clean(s: Option<&str>) -> Option<&str> {
        s.map(str::trim).filter(|s| !s.is_empty())
    }
    let head = [clean(title), clean(album)].into_iter().flatten().collect::<Vec<_>>().join(" - ");
    let name = match (head.is_empty(), clean(artist)) {
        (true, None) => return (file_stem.trim().to_owned(), false),
        (true, Some(artist)) => artist.to_owned(),
        (false, None) => head,
        (false, Some(artist)) => format!("{head} by {artist}"),
    };
    (name, true)
}

pub fn guess_role(file_name: &str) -> TrackRole {
    let name = file_name.to_lowercase();
    let has = |words: &[&str]| words.iter().any(|w| name.contains(w));
    if has(&["drum", "perc"]) {
        TrackRole::Drums
    } else if has(&["bass"]) {
        TrackRole::Bass
    } else if has(&["guitar", "gtr"]) {
        TrackRole::Guitar
    } else {
        TrackRole::Other
    }
}

#[derive(Clone, Debug)]
pub struct FormValues {
    pub name: String,
    pub bpm: String,
    pub time_sig: String,
    pub reference_hz: String,
    pub tuning: String,
    pub instrument: Instrument,
    pub capo: u8,
    pub main_path: PathBuf,
    pub extra: Vec<(PathBuf, TrackRole)>,
}

#[derive(Clone, Debug)]
pub struct Validated {
    pub name: String,
    pub bpm: f64,
    pub time_sig: TimeSignature,
    pub reference_hz: f32,
    pub tuning: Tuning,
    pub capo: u8,
    pub main_path: PathBuf,
    pub extra: Vec<(PathBuf, TrackRole)>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct FormErrors {
    pub name: Option<String>,
    pub bpm: Option<String>,
    pub time_sig: Option<String>,
    pub reference: Option<String>,
    pub tuning: Option<String>,
    pub capo: Option<String>,
    pub main_file: Option<String>,
}

fn parse_time_sig(text: &str) -> Result<TimeSignature, String> {
    let bad = || "Use the form 4/4 (denominator 2, 4, 8 or 16)".to_owned();
    let (n, d) = text.split_once('/').ok_or_else(bad)?;
    let sig = TimeSignature::new(n.trim().parse().map_err(|_| bad())?, d.trim().parse().map_err(|_| bad())?);
    if sig.is_valid() && matches!(sig.denominator, 2 | 4 | 8 | 16) {
        Ok(sig)
    } else {
        Err(bad())
    }
}

fn parse_ranged(text: &str, lo: f64, hi: f64, unit: &str) -> Result<f64, String> {
    text.trim()
        .parse::<f64>()
        .ok()
        .filter(|v| (lo..=hi).contains(v))
        .ok_or_else(|| format!("Enter a number from {lo} to {hi} {unit}"))
}

fn stem(path: &Path) -> String {
    path.file_stem().map_or_else(String::new, |s| s.to_string_lossy().into_owned())
}

impl FormValues {
    #[allow(clippy::result_large_err)]
    pub fn validate(&self, main_readable: bool) -> Result<Validated, FormErrors> {
        let mut errors = FormErrors::default();
        let name = self.name.trim().to_owned();
        errors.name = name.is_empty().then(|| "Enter a name".to_owned());
        let bpm = parse_ranged(&self.bpm, MIN_BPM, MAX_BPM, "BPM").map_err(|e| errors.bpm = Some(e)).ok();
        let time_sig = parse_time_sig(&self.time_sig).map_err(|e| errors.time_sig = Some(e)).ok();
        let reference_hz = parse_ranged(&self.reference_hz, 400.0, 480.0, "Hz")
            .ok()
            .and_then(|hz| PitchRef::new(hz as f32).ok())
            .map(|p| p.a4_hz());
        if reference_hz.is_none() {
            errors.reference = Some("Enter a frequency from 400 to 480 Hz".to_owned());
        }
        let tuning = parse_tuning(&self.tuning, self.instrument).map_err(|e| errors.tuning = Some(e)).ok();
        errors.capo = (self.capo > MAX_CAPO).then(|| format!("Capo must be 0 to {MAX_CAPO}"));
        errors.main_file = if self.main_path.as_os_str().is_empty() {
            Some("Choose an audio file".to_owned())
        } else {
            (!main_readable).then(|| "This audio file can't be read".to_owned())
        };
        match (bpm, time_sig, reference_hz, tuning, errors == FormErrors::default()) {
            (Some(bpm), Some(time_sig), Some(reference_hz), Some(tuning), true) => Ok(Validated {
                name,
                bpm,
                time_sig,
                reference_hz,
                tuning,
                capo: self.capo,
                main_path: self.main_path.clone(),
                extra: self.extra.clone(),
            }),
            _ => Err(errors),
        }
    }
}

pub fn build_project(v: &Validated, audio_len_sec: f64) -> Project {
    let track = |path: &Path, role| {
        let mut t = Track::new(1, stem(path), role);
        t.audio_path = Some(path.to_path_buf());
        t
    };
    let mut project = Project::new(track(&v.main_path, TrackRole::Guitar), audio_len_sec, v.bpm, v.time_sig);
    for (path, role) in &v.extra {
        project.add_track(track(path, *role));
    }
    project.tuning = v.tuning.clone();
    project.capo = v.capo;
    project.reference_hz = v.reference_hz;
    project
}

#[cfg(test)]
mod tests {
    use super::*;

    fn guitar(text: &str) -> Result<Tuning, String> {
        parse_tuning(text, Instrument::Guitar)
    }

    fn values() -> FormValues {
        FormValues {
            name: "Song".into(),
            bpm: "120".into(),
            time_sig: "4/4".into(),
            reference_hz: "440".into(),
            tuning: "E A D G B e".into(),
            instrument: Instrument::Guitar,
            capo: 0,
            main_path: "/a/song.wav".into(),
            extra: vec![("/a/bass.wav".into(), TrackRole::Bass)],
        }
    }

    #[test]
    fn name_from_tags_or_file_stem() {
        let n = |t, al, ar| project_name(t, al, ar, "stem");
        assert_eq!(n(Some("T"), Some("A"), Some("X")), ("T - A by X".into(), true));
        assert_eq!(n(Some(" T "), None, Some("X")), ("T by X".into(), true));
        assert_eq!(n(None, Some("A"), None), ("A".into(), true));
        assert_eq!(n(None, None, Some("X")), ("X".into(), true));
        assert_eq!(n(Some("  "), Some(""), None), ("stem".into(), false));
    }

    #[test]
    fn tuning_parse_and_names() {
        let t = guitar("E A D G B e").unwrap();
        assert_eq!((t.name.as_str(), t.open_strings.clone()), ("Standard", Tuning::standard().open_strings));
        assert_eq!(guitar("EADGBE").unwrap().name, "Standard");
        assert_eq!(guitar("DADGAD").unwrap().open_strings, [38, 45, 50, 55, 57, 62]);
        assert_eq!(guitar("DADGAD").unwrap().name, "DADGAD");
        assert_eq!(guitar("D A D G B e").unwrap().name, "Drop D");
        assert_eq!(guitar("E A D G B C").unwrap().name, "Custom");
        assert_eq!(guitar("E2 A2 D3 G3 B3 E4").unwrap().name, "Standard");
        assert_eq!(guitar("Eb Ab Db Gb Bb eb").unwrap().name, "Half-step down");
        assert_eq!(guitar("d# g# c# f# a# d#").unwrap().name, "Half-step down");
        assert_eq!(guitar("EADGBb").unwrap().open_strings.len(), 6);
        let bass = parse_tuning("E A D G", Instrument::Bass).unwrap();
        assert_eq!(bass.open_strings, [28, 33, 38, 43]);
        assert_eq!(bass.name, "Bass 4-string");
    }

    #[test]
    fn tuning_parse_errors() {
        for bad in ["", "E A D", "E A D G B X", "E A D G B E E E E", "E A D G B E#9x", "e2 a2 d3 g3"] {
            assert!(guitar(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn presets_round_trip() {
        for instrument in [Instrument::Guitar, Instrument::Bass] {
            for (label, text) in preset_texts(instrument) {
                let t = parse_tuning(&text, instrument).unwrap();
                assert_eq!(tuning_text(&t), text, "{label}");
            }
        }
        assert_eq!(preset_texts(Instrument::Guitar).len(), 6);
        assert_eq!(preset_texts(Instrument::Guitar)[0].1, "E A D G B e");
        assert_eq!(tuning_text(&guitar("EADGBE").unwrap()), "E A D G B e");
    }

    #[test]
    fn range_boundaries() {
        let check = |f: fn(&mut FormValues, &str), ok: &[&str], bad: &[&str]| {
            for (cases, expect) in [(ok, true), (bad, false)] {
                for c in cases {
                    let mut v = values();
                    f(&mut v, c);
                    assert_eq!(v.validate(true).is_ok(), expect, "{c}");
                }
            }
        };
        check(|v, s| v.bpm = s.into(), &["10", "500", "93.5"], &["9.9", "501", "abc", ""]);
        check(|v, s| v.reference_hz = s.into(), &["400", "480"], &["399.9", "480.1", "x"]);
        check(|v, s| v.time_sig = s.into(), &["3/4", "6/8", "7/16", "2/2"], &["4/3", "0/4", "4/32", "4", "65/4"]);
        check(|v, s| v.name = s.into(), &["x"], &["", "   "]);
        let mut v = values();
        v.capo = 12;
        assert!(v.validate(true).is_ok());
        v.capo = 13;
        assert!(v.validate(true).unwrap_err().capo.is_some());
    }

    #[test]
    fn errors_are_per_field() {
        let mut v = values();
        v.bpm = "1".into();
        v.tuning = "zz".into();
        let e = v.validate(false).unwrap_err();
        assert!(e.bpm.is_some() && e.tuning.is_some() && e.main_file.is_some());
        assert!(e.name.is_none() && e.time_sig.is_none() && e.reference.is_none());
    }

    #[test]
    fn roles_from_file_names() {
        let roles: Vec<_> = ["Drums.wav", "my_bass.flac", "Lead GTR.mp3", "backing.wav", "Percussion.wav"].map(guess_role).to_vec();
        assert_eq!(roles, [TrackRole::Drums, TrackRole::Bass, TrackRole::Guitar, TrackRole::Other, TrackRole::Drums]);
    }

    #[test]
    fn built_project_is_valid() {
        let mut v = values();
        v.capo = 2;
        v.reference_hz = "442".into();
        let p = build_project(&v.validate(true).unwrap(), 30.0);
        p.validate().unwrap();
        assert_eq!(p.tracks.len(), 2);
        assert_eq!(p.source_track, p.tracks[0].id);
        assert!(p.tracks.iter().all(|t| t.audio_path.is_some()));
        assert_eq!((p.capo, p.reference_hz, p.tracks[1].role), (2, 442.0, TrackRole::Bass));
        assert_eq!(p.tracks[0].name, "song");
    }
}
