//! Plays one or more audio files mixed together and prints the transport position.
//!
//! Usage: play [--seek SECONDS] PATH[:OFFSET_MS]...
//!
//! `OFFSET_MS` may be negative; it shifts the file in project time like a track offset.

use std::error::Error;
use std::path::PathBuf;
use std::process::ExitCode;
use std::thread;
use std::time::Duration;

use fretscribe_audio::{decode_files, Player, PreparedAudio, PreparedTrack};

const USAGE: &str = "usage: play [--seek SECONDS] PATH[:OFFSET_MS]...";
const REPORT_EVERY: Duration = Duration::from_millis(250);

#[derive(Debug, PartialEq)]
struct Args {
    seek_sec: f64,
    inputs: Vec<(PathBuf, f64)>,
}

fn parse_input(arg: &str) -> (PathBuf, f64) {
    if let Some((path, suffix)) = arg.rsplit_once(':') {
        if let Ok(offset_ms) = suffix.parse::<i64>() {
            return (PathBuf::from(path), offset_ms as f64 / 1000.0);
        }
    }
    (PathBuf::from(arg), 0.0)
}

fn parse_args(args: impl IntoIterator<Item = String>) -> Result<Args, String> {
    let mut args = args.into_iter();
    let mut parsed = Args {
        seek_sec: 0.0,
        inputs: Vec::new(),
    };
    while let Some(arg) = args.next() {
        if arg == "--seek" {
            let value = args.next().ok_or("--seek needs a value in seconds")?;
            parsed.seek_sec = value
                .parse::<f64>()
                .ok()
                .filter(|v| v.is_finite())
                .ok_or_else(|| format!("invalid seek position: {value}"))?;
        } else {
            parsed.inputs.push(parse_input(&arg));
        }
    }
    if parsed.inputs.is_empty() {
        return Err("no input files".to_owned());
    }
    Ok(parsed)
}

fn run(args: Args) -> Result<(), Box<dyn Error>> {
    let player = Player::new()?;
    let paths: Vec<&PathBuf> = args.inputs.iter().map(|(path, _)| path).collect();
    let decoded = decode_files(&paths);

    let mut tracks = Vec::with_capacity(args.inputs.len());
    for (id, ((path, offset_sec), audio)) in (1u64..).zip(args.inputs.iter().zip(decoded)) {
        let audio = audio.map_err(|e| format!("{}: {e}", path.display()))?;
        println!(
            "track {id}: {} ({} Hz, {} ch, {:.2} s, offset {:+.0} ms)",
            path.display(),
            audio.sample_rate,
            audio.channels,
            audio.duration_sec,
            offset_sec * 1000.0
        );
        tracks.push((id, *offset_sec, audio));
    }

    let rate = player.output_rate();
    let prepared = thread::scope(|scope| {
        let workers: Vec<_> = tracks
            .iter()
            .map(|(_, _, audio)| scope.spawn(move || PreparedAudio::new(audio, rate)))
            .collect();
        workers
            .into_iter()
            .map(|worker| worker.join().map_err(|_| "a preparation thread panicked".to_owned())?.map_err(|e| e.to_string()))
            .collect::<Result<Vec<_>, String>>()
    })?;
    player.set_prepared_tracks(
        tracks
            .iter()
            .zip(prepared)
            .map(|(&(id, offset_sec, _), audio)| PreparedTrack {
                id,
                audio,
                offset_sec,
                volume: 1.0,
                muted: false,
                solo: false,
            })
            .collect(),
    )?;
    player.seek(args.seek_sec);
    player.play();
    let duration = player.duration_sec();

    while player.is_playing() {
        let levels: Vec<String> = player
            .track_peak_levels()
            .iter()
            .map(|(id, peak)| format!("#{id} {peak:.2}"))
            .collect();
        println!(
            "{:8.3} / {duration:.3} s   {}",
            player.position_sec(),
            levels.join("  ")
        );
        thread::sleep(REPORT_EVERY);
    }
    println!("{:8.3} / {duration:.3} s   done", player.position_sec());
    Ok(())
}

fn main() -> ExitCode {
    let args = match parse_args(std::env::args().skip(1)) {
        Ok(args) => args,
        Err(message) => {
            eprintln!("{message}\n{USAGE}");
            return ExitCode::from(2);
        }
    };
    match run(args) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("error: {e}");
            ExitCode::FAILURE
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(args: &[&str]) -> Result<Args, String> {
        parse_args(args.iter().map(|s| (*s).to_owned()))
    }

    #[test]
    fn offsets_are_optional_and_may_be_negative() {
        let args = parse(&["a.wav", "b.flac:250", "c.mp3:-1500"]).unwrap();
        assert_eq!(
            args.inputs,
            [
                (PathBuf::from("a.wav"), 0.0),
                (PathBuf::from("b.flac"), 0.25),
                (PathBuf::from("c.mp3"), -1.5),
            ]
        );
        assert_eq!(args.seek_sec, 0.0);
    }

    #[test]
    fn a_drive_letter_colon_is_not_an_offset() {
        let args = parse(&["C:\\music\\a.wav", "C:\\music\\b.wav:100"]).unwrap();
        assert_eq!(args.inputs[0], (PathBuf::from("C:\\music\\a.wav"), 0.0));
        assert_eq!(args.inputs[1], (PathBuf::from("C:\\music\\b.wav"), 0.1));
    }

    #[test]
    fn seek_and_errors() {
        assert_eq!(parse(&["--seek", "12.5", "a.wav"]).unwrap().seek_sec, 12.5);
        assert!(parse(&["--seek"]).is_err());
        assert!(parse(&["--seek", "soon", "a.wav"]).is_err());
        assert!(parse(&["--seek", "3"]).is_err());
        assert!(parse(&[]).is_err());
    }
}
