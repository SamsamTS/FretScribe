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
    for bad in ["", "E A D", "E A D G B X", "E A D G B E E E E", "E A D G B E#9x", "E4 B3 G3 D3 A2 E2"] {
        assert!(guitar(bad).is_err(), "{bad}");
    }
}

#[test]
fn commit_normalisers() {
    assert_eq!(normalise_tuning("DADGAD", Instrument::Guitar).as_deref(), Some("D A D G A d"));
    assert_eq!(normalise_tuning("zz", Instrument::Guitar), None);
    assert_eq!(normalise_number(" 93 ", 1).as_deref(), Some("93.0"));
    assert_eq!(normalise_number("abc", 1), None);
    assert_eq!(normalise_time_sig("6 / 8").as_deref(), Some("6/8"));
    assert_eq!(normalise_time_sig("4/3"), None);
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
