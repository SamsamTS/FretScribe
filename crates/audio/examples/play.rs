//! Plays one or more audio files mixed together and prints the transport position.
//!
//! Usage: play [options] PATH[:OFFSET_MS]...
//!
//! Options: --seek SEC, --speed X (0.25-2.0), --mode preserve|change, --region START END, --loop.
//! `OFFSET_MS` may be negative; it shifts the file in project time like a track offset.
//!
//! While playing, type a command and press Enter to change things live, which is the way to
//! listen for clicks when switching: see `HELP`.

use std::error::Error;
use std::path::PathBuf;
use std::process::ExitCode;
use std::sync::mpsc::{self, RecvTimeoutError};
use std::thread;
use std::time::Duration;

use fretscribe_audio::{
    decode_files, Player, PreparedAudio, PreparedTrack, SpeedMode, MAX_SPEED, MIN_SPEED,
};

const USAGE: &str = "usage: play [--seek SEC] [--speed X] [--mode preserve|change] [--region START END] [--loop] PATH[:OFFSET_MS]...";
const HELP: &str = "commands: speed X | mode preserve|change | loop on|off | region START END | region off | seek SEC | play | pause | stop | quit";
const REPORT_EVERY: Duration = Duration::from_millis(250);

#[derive(Debug, PartialEq)]
struct Args {
    seek_sec: f64,
    speed: f32,
    mode: SpeedMode,
    region: Option<(f64, f64)>,
    looping: bool,
    inputs: Vec<(PathBuf, f64)>,
}

fn number(text: &str, what: &str) -> Result<f64, String> {
    text.parse::<f64>()
        .ok()
        .filter(|v| v.is_finite())
        .ok_or_else(|| format!("invalid {what}: {text}"))
}

fn speed_value(text: &str) -> Result<f32, String> {
    let speed = number(text, "speed")? as f32;
    if (MIN_SPEED..=MAX_SPEED).contains(&speed) {
        Ok(speed)
    } else {
        Err(format!("speed must be between {MIN_SPEED} and {MAX_SPEED}"))
    }
}

fn mode_value(text: &str) -> Result<SpeedMode, String> {
    match text {
        "preserve" => Ok(SpeedMode::PreservePitch),
        "change" => Ok(SpeedMode::ChangePitch),
        _ => Err(format!("mode must be preserve or change, not {text}")),
    }
}

fn region_value(start: &str, end: &str) -> Result<(f64, f64), String> {
    let (start, end) = (number(start, "region start")?, number(end, "region end")?);
    if start < 0.0 || end <= start {
        return Err("a region needs 0 <= START < END".to_owned());
    }
    Ok((start, end))
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
        speed: 1.0,
        mode: SpeedMode::default(),
        region: None,
        looping: false,
        inputs: Vec::new(),
    };
    let value = |flag: &str, args: &mut dyn Iterator<Item = String>| {
        args.next().ok_or_else(|| format!("{flag} needs a value"))
    };
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--seek" => parsed.seek_sec = number(&value(&arg, &mut args)?, "seek position")?,
            "--speed" => parsed.speed = speed_value(&value(&arg, &mut args)?)?,
            "--mode" => parsed.mode = mode_value(&value(&arg, &mut args)?)?,
            "--region" => {
                let start = value(&arg, &mut args)?;
                let end = value(&arg, &mut args)?;
                parsed.region = Some(region_value(&start, &end)?);
            }
            "--loop" => parsed.looping = true,
            flag if flag.starts_with("--") => return Err(format!("unknown option {flag}")),
            _ => parsed.inputs.push(parse_input(&arg)),
        }
    }
    if parsed.inputs.is_empty() {
        return Err("no input files".to_owned());
    }
    Ok(parsed)
}

#[derive(Debug, PartialEq)]
enum Command {
    Speed(f32),
    Mode(SpeedMode),
    Loop(bool),
    Region(Option<(f64, f64)>),
    Seek(f64),
    Play,
    Pause,
    Stop,
    Quit,
}

fn parse_command(line: &str) -> Result<Command, String> {
    let words: Vec<&str> = line.split_whitespace().collect();
    match words.as_slice() {
        ["speed", x] => Ok(Command::Speed(speed_value(x)?)),
        ["mode", m] => Ok(Command::Mode(mode_value(m)?)),
        ["loop", "on"] => Ok(Command::Loop(true)),
        ["loop", "off"] => Ok(Command::Loop(false)),
        ["region", "off"] => Ok(Command::Region(None)),
        ["region", start, end] => Ok(Command::Region(Some(region_value(start, end)?))),
        ["seek", sec] => Ok(Command::Seek(number(sec, "seek position")?)),
        ["play"] => Ok(Command::Play),
        ["pause"] => Ok(Command::Pause),
        ["stop"] => Ok(Command::Stop),
        ["quit"] | ["q"] => Ok(Command::Quit),
        _ => Err(format!("unknown command: {}", line.trim())),
    }
}

/// What the player was last told, shown in the status line.
struct View {
    speed: f32,
    mode: SpeedMode,
    region: Option<(f64, f64)>,
    looping: bool,
}

impl View {
    fn describe(&self) -> String {
        let mode = match self.mode {
            SpeedMode::PreservePitch => "preserve",
            SpeedMode::ChangePitch => "change",
        };
        let region = self
            .region
            .map_or_else(String::new, |(a, b)| format!(" region {a:.1}-{b:.1}"));
        let looping = if self.looping { " loop" } else { "" };
        format!("x{:.2} {mode}{region}{looping}", self.speed)
    }
}

fn load(player: &Player, args: &Args) -> Result<(), Box<dyn Error>> {
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
            .map(|worker| {
                worker
                    .join()
                    .map_err(|_| "a preparation thread panicked".to_owned())?
                    .map_err(|e| e.to_string())
            })
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
    Ok(())
}

fn spawn_stdin_reader() -> mpsc::Receiver<String> {
    let (tx, rx) = mpsc::channel();
    thread::spawn(move || {
        for line in std::io::stdin().lines().map_while(Result::ok) {
            if tx.send(line).is_err() {
                break;
            }
        }
    });
    rx
}

fn run(args: Args) -> Result<(), Box<dyn Error>> {
    let player = Player::new()?;
    load(&player, &args)?;

    let mut view = View {
        speed: args.speed,
        mode: args.mode,
        region: args.region,
        looping: args.looping,
    };
    player.set_speed_mode(view.mode);
    player.set_playback_speed(view.speed);
    player.set_region(view.region);
    player.set_loop(view.looping);
    player.seek(args.seek_sec);
    player.play();
    let duration = player.duration_sec();
    println!("{HELP}");

    let commands = spawn_stdin_reader();
    let mut input_open = true;
    let mut paused = false;
    loop {
        if !player.is_playing() && !paused {
            break;
        }
        let levels: Vec<String> = player
            .track_peak_levels()
            .iter()
            .map(|(id, peak)| format!("#{id} {peak:.2}"))
            .collect();
        println!(
            "{:8.3} / {duration:.3} s   [{}]   {}",
            player.position_sec(),
            view.describe(),
            levels.join("  ")
        );

        let line = if input_open {
            match commands.recv_timeout(REPORT_EVERY) {
                Ok(line) => line,
                Err(RecvTimeoutError::Timeout) => continue,
                Err(RecvTimeoutError::Disconnected) => {
                    input_open = false;
                    continue;
                }
            }
        } else {
            thread::sleep(REPORT_EVERY);
            continue;
        };
        if line.trim().is_empty() {
            continue;
        }
        match parse_command(&line) {
            Ok(Command::Speed(x)) => {
                view.speed = x;
                player.set_playback_speed(x);
            }
            Ok(Command::Mode(mode)) => {
                view.mode = mode;
                player.set_speed_mode(mode);
            }
            Ok(Command::Loop(on)) => {
                view.looping = on;
                player.set_loop(on);
            }
            Ok(Command::Region(region)) => {
                view.region = region;
                player.set_region(region);
            }
            Ok(Command::Seek(sec)) => player.seek(sec),
            Ok(Command::Play) => {
                paused = false;
                player.play();
            }
            Ok(Command::Pause) => {
                paused = true;
                player.pause();
            }
            Ok(Command::Stop) => {
                paused = true;
                player.stop();
            }
            Ok(Command::Quit) => break,
            Err(message) => println!("{message}\n{HELP}"),
        }
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
        assert_eq!((args.speed, args.mode, args.region, args.looping), (1.0, SpeedMode::PreservePitch, None, false));
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
        assert!(parse(&["--bogus", "a.wav"]).is_err());
    }

    #[test]
    fn playback_options() {
        let args = parse(&["--speed", "0.5", "--mode", "change", "--region", "10", "20.5", "--loop", "a.wav"]).unwrap();
        assert_eq!(args.speed, 0.5);
        assert_eq!(args.mode, SpeedMode::ChangePitch);
        assert_eq!(args.region, Some((10.0, 20.5)));
        assert!(args.looping);
        assert_eq!(args.inputs.len(), 1);

        assert!(parse(&["--speed", "3", "a.wav"]).is_err());
        assert!(parse(&["--speed", "0.1", "a.wav"]).is_err());
        assert!(parse(&["--mode", "tape", "a.wav"]).is_err());
        assert!(parse(&["--region", "10", "a.wav"]).is_err());
        assert!(parse(&["--region", "20", "10", "a.wav"]).is_err());
        assert!(parse(&["--region", "-1", "5", "a.wav"]).is_err());
    }

    #[test]
    fn live_commands() {
        assert_eq!(parse_command("speed 0.75"), Ok(Command::Speed(0.75)));
        assert_eq!(parse_command("  mode   change "), Ok(Command::Mode(SpeedMode::ChangePitch)));
        assert_eq!(parse_command("mode preserve"), Ok(Command::Mode(SpeedMode::PreservePitch)));
        assert_eq!(parse_command("loop on"), Ok(Command::Loop(true)));
        assert_eq!(parse_command("loop off"), Ok(Command::Loop(false)));
        assert_eq!(parse_command("region 5 8.5"), Ok(Command::Region(Some((5.0, 8.5)))));
        assert_eq!(parse_command("region off"), Ok(Command::Region(None)));
        assert_eq!(parse_command("seek 42"), Ok(Command::Seek(42.0)));
        assert_eq!(parse_command("play"), Ok(Command::Play));
        assert_eq!(parse_command("pause"), Ok(Command::Pause));
        assert_eq!(parse_command("stop"), Ok(Command::Stop));
        assert_eq!(parse_command("q"), Ok(Command::Quit));
        for bad in ["speed", "speed fast", "speed 5", "loop maybe", "region 9 3", "dance"] {
            assert!(parse_command(bad).is_err(), "{bad}");
        }
    }
}
