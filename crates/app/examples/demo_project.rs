//! Writes a synthetic 20 s song (clicks plus a tone melody) and a project file using it:
//! `cargo run -p fretscribe --example demo_project -- <out dir>`, then open the `.fretscribe` file.

use std::f32::consts::TAU;
use std::path::PathBuf;

use fretscribe_core::{Note, Project, ProjectFile, TempoMap, TimeSignature, Track, TrackRole};

fn wav(path: &std::path::Path, samples: &[f32], rate: u32) -> std::io::Result<()> {
    let data: Vec<u8> = samples.iter().flat_map(|s| ((s.clamp(-1.0, 1.0) * 32000.0) as i16).to_le_bytes()).collect();
    let mut out = Vec::new();
    out.extend(b"RIFF");
    out.extend((36 + data.len() as u32).to_le_bytes());
    out.extend(b"WAVEfmt ");
    out.extend([16, 0, 0, 0, 1, 0, 1, 0]);
    out.extend(rate.to_le_bytes());
    out.extend((rate * 2).to_le_bytes());
    out.extend([2, 0, 16, 0]);
    out.extend(b"data");
    out.extend((data.len() as u32).to_le_bytes());
    out.extend(data);
    std::fs::write(path, out)
}

fn main() -> std::io::Result<()> {
    let dir = PathBuf::from(std::env::args().nth(1).unwrap_or_else(|| ".".into()));
    std::fs::create_dir_all(&dir)?;
    let rate = 44_100_u32;
    let (bpm, secs) = (120.0_f32, 20.0_f32);
    let melody = [64.0_f32, 67.0, 71.0, 69.0, 66.0, 64.0, 62.0, 64.0];
    let mut guitar = vec![0.0_f32; (secs * rate as f32) as usize];
    let mut drums = guitar.clone();
    let beat = 60.0 / bpm;
    for (i, s) in guitar.iter_mut().enumerate() {
        let t = i as f32 / rate as f32;
        let k = (t / beat) as usize;
        let local = t - k as f32 * beat;
        let f = 440.0 * ((melody[k % melody.len()] - 69.0) / 12.0).exp2();
        *s = 0.4 * (TAU * f * t).sin() * (-local * 3.0).exp();
    }
    for (i, s) in drums.iter_mut().enumerate() {
        let local = (i as f32 / rate as f32) % beat;
        *s = 0.6 * (TAU * 120.0 * local).sin() * (-local * 40.0).exp();
    }
    wav(&dir.join("guitar.wav"), &guitar, rate)?;
    wav(&dir.join("drums.wav"), &drums, rate)?;

    let mut main = Track::new(1, "guitar", TrackRole::Guitar);
    main.audio_path = Some(dir.join("guitar.wav").canonicalize()?);
    let mut project = Project::new(main, f64::from(secs), f64::from(bpm), TimeSignature { numerator: 4, denominator: 4 });
    let mut drum_track = Track::new(2, "drums", TrackRole::Drums);
    drum_track.audio_path = Some(dir.join("drums.wav").canonicalize()?);
    project.add_track(drum_track);
    let map = TempoMap::new(&project.measures);
    let mut notes = Vec::new();
    for k in 0..16_u64 {
        let start = map.sec_to_pos(k as f64 * f64::from(beat));
        let end = map.sec_to_pos((k as f64 + 0.9) * f64::from(beat));
        notes.push(Note::new(k + 1, melody[k as usize % melody.len()] as u8, start, end));
    }
    project.notes = notes;
    fretscribe_core::save_project(&dir.join("demo.fretscribe"), &ProjectFile::new(project)).map_err(std::io::Error::other)?;
    println!("wrote {}", dir.join("demo.fretscribe").display());
    Ok(())
}
