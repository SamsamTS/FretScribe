//! Prints the waveform peaks of an audio file (WAV or anything symphonia decodes).
//!
//! cargo run --release -p fretscribe-analysis --example waveform_peaks -- song.wav \
//!     [--start 0] [--seconds 10] [--columns 40]

use std::path::PathBuf;
use std::process::exit;
use std::time::Instant;

use fretscribe_analysis::WaveformPyramid;

const USAGE: &str =
    "usage: waveform_peaks <audio file> [--start sec] [--seconds sec] [--columns n]";

fn fail(message: &str) -> ! {
    eprintln!("{message}\n{USAGE}");
    exit(2)
}

fn number<T: std::str::FromStr>(name: &str, value: Option<String>) -> T {
    let Some(value) = value else {
        fail(&format!("missing value for {name}"))
    };
    value
        .parse()
        .unwrap_or_else(|_| fail(&format!("invalid number for {name}: {value}")))
}

fn main() {
    let mut audio: Option<PathBuf> = None;
    let (mut start, mut seconds, mut columns) = (0.0f64, None::<f64>, 40usize);

    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--start" => start = number(&arg, args.next()),
            "--seconds" => seconds = Some(number(&arg, args.next())),
            "--columns" => columns = number(&arg, args.next()),
            _ if !arg.starts_with("--") && audio.is_none() => audio = Some(PathBuf::from(arg)),
            _ => fail(&format!("unknown argument {arg}")),
        }
    }
    let Some(audio) = audio else {
        fail("no audio file given")
    };

    let decoded = fretscribe_audio::decode_file(&audio).unwrap_or_else(|e| {
        eprintln!("{}: {e}", audio.display());
        exit(1)
    });

    let timer = Instant::now();
    let pyramid = WaveformPyramid::build(&decoded.mono, decoded.sample_rate);
    eprintln!(
        "{:.1} s of audio, pyramid built in {:.2?}",
        pyramid.duration_sec(),
        timer.elapsed()
    );

    let end = seconds.map_or(pyramid.duration_sec(), |s| start + s);
    let peaks = pyramid.peaks((start, end), columns);
    let step = (end - start) / columns.max(1) as f64;
    println!(
        "{:>9}  {:>8}  {:>8}  {:>8}",
        "time (s)", "min", "max", "rms"
    );
    for (i, (min, max, rms)) in peaks.iter().enumerate() {
        let time = start + i as f64 * step;
        println!("{time:>9.3}  {min:>8.4}  {max:>8.4}  {rms:>8.4}");
    }
}
