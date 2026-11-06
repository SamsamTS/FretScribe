//! Prints the estimated reference pitch of an audio file.
//!
//! cargo run --release -p fretscribe-analysis --example pitchref -- song.wav [--start 0 --end 30]

use std::process::exit;

use fretscribe_analysis::pitchref::{estimate_reference, PitchRefOptions};

const USAGE: &str = "usage: pitchref <audio file> [--start sec --end sec]";

fn fail(message: &str) -> ! {
    eprintln!("{message}\n{USAGE}");
    exit(2)
}

fn main() {
    let (mut file, mut start, mut end) = (None, None::<f64>, None::<f64>);
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        let mut number =
            || args.next().and_then(|v| v.parse().ok()).unwrap_or_else(|| fail("invalid number"));
        match arg.as_str() {
            "--start" => start = Some(number()),
            "--end" => end = Some(number()),
            _ if !arg.starts_with("--") && file.is_none() => file = Some(arg),
            _ => fail(&format!("unknown argument {arg}")),
        }
    }
    let Some(file) = file else { fail("no audio file given") };
    let audio = fretscribe_audio::decode_file(&file).unwrap_or_else(|e| {
        eprintln!("{file}: {e}");
        exit(1)
    });
    let region = (start.is_some() || end.is_some())
        .then(|| (start.unwrap_or(0.0), end.unwrap_or(audio.duration_sec)));
    let e = estimate_reference(&audio.mono, audio.sample_rate, region, PitchRefOptions::default());
    println!("{:+.1} cents  A4 = {:.2} Hz  confidence {:.2}", e.cents, e.a4_hz, e.confidence);
}
