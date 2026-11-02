//! Renders a spectrogram of an audio file to a PNG for visual inspection.
//!
//! cargo run --release -p fretscribe-analysis --example spectrogram_png -- song.flac \
//!     [--out file.png] [--lut Heat] [--black 0.25] [--white 0.9] [--gamma 0.7] \
//!     [--start 30] [--seconds 10] [--width 1600] [--height 640]

use std::path::{Path, PathBuf};
use std::process::exit;
use std::time::Instant;

use fretscribe_analysis::{compute_spectrogram, discover_luts, Levels, Lut, SpectrogramParams};
use fretscribe_core::PitchRef;

const USAGE: &str = "usage: spectrogram_png <audio file> [--out file.png] [--lut name] \
[--black v] [--white v] [--gamma v] [--start sec] [--seconds sec] [--width px] [--height px]";

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
    let mut out = PathBuf::from("spectrogram.png");
    let (mut lut_name, mut black, mut white, mut gamma) = (None, 0.0f32, 1.0f32, 1.0f32);
    let (mut start, mut seconds) = (0.0f64, None::<f64>);
    let (mut width, mut height) = (1600usize, 640usize);

    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--out" => out = PathBuf::from(number::<String>(&arg, args.next())),
            "--lut" => lut_name = Some(number::<String>(&arg, args.next())),
            "--black" => black = number(&arg, args.next()),
            "--white" => white = number(&arg, args.next()),
            "--gamma" => gamma = number(&arg, args.next()),
            "--start" => start = number(&arg, args.next()),
            "--seconds" => seconds = Some(number(&arg, args.next())),
            "--width" => width = number(&arg, args.next()),
            "--height" => height = number(&arg, args.next()),
            _ if !arg.starts_with("--") && audio.is_none() => audio = Some(PathBuf::from(arg)),
            _ => fail(&format!("unknown argument {arg}")),
        }
    }
    let Some(audio) = audio else {
        fail("no audio file given")
    };

    let lut_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../LUT");
    let mut luts = discover_luts(&lut_dir);
    if luts.is_empty() {
        luts.push(Lut::builtin());
    }
    let lut = match &lut_name {
        None => &luts[0],
        Some(name) => luts.iter().find(|l| &l.name == name).unwrap_or_else(|| {
            let names: Vec<&str> = luts.iter().map(|l| l.name.as_str()).collect();
            fail(&format!(
                "unknown palette {name}; available: {}",
                names.join(", ")
            ))
        }),
    };

    let decoded = fretscribe_audio::decode_file(&audio).unwrap_or_else(|e| {
        eprintln!("{}: {e}", audio.display());
        exit(1)
    });
    let params = SpectrogramParams::default();
    let timer = Instant::now();
    let spectrogram = compute_spectrogram(&decoded.mono, decoded.sample_rate, &params);
    eprintln!(
        "{:.1} s of audio in {:.2?}",
        decoded.duration_sec,
        timer.elapsed()
    );

    let end = seconds.map_or(decoded.duration_sec, |s| start + s);
    let rgba = spectrogram.render_rgba(
        (start, end),
        width,
        height,
        (params.min_midi, params.max_midi),
        &PitchRef::default(),
        lut,
        Levels::new(black, white),
        gamma,
    );
    let saved = image::RgbaImage::from_raw(width as u32, height as u32, rgba)
        .ok_or("buffer size mismatch")
        .map_err(|e| e.to_string())
        .and_then(|img| img.save(&out).map_err(|e| e.to_string()));
    match saved {
        Ok(()) => println!("{}", out.display()),
        Err(e) => {
            eprintln!("{}: {e}", out.display());
            exit(1)
        }
    }
}
