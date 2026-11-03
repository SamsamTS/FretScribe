//! Renders a part of an audio file as a waveform PNG for visual inspection. `harness = false`:
//!
//! cargo test --release -p fretscribe-analysis --test export_waveform_png -- --audio "D:\Projects\FretScribe\Mockup\guitar.mp3" --start 12.5 --seconds 4 --lut Heat
//!
//! Without arguments it only prints the usage, so a plain `cargo test` passes.

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::process::exit;

use fretscribe_analysis::waveform::render_rgba;
use fretscribe_analysis::{discover_luts, Lut, WaveformPyramid};

const USAGE: &str = "\
usage: export_waveform_png --audio <path> --seconds <n> [--start <sec>] [--lut <Classic|Heat>]
                           [--width <px>] [--height <px>] [--out <path>]
  --audio    audio file (required)
  --seconds  number of seconds to render, > 0; clamped to the file length (required)
  --start    start in seconds, >= 0 (default 0)
  --lut      palette, case-insensitive: Classic or Heat (default: the spectrogram's default)
  --width    image width in pixels (default 1600)
  --height   image height in pixels (default 300)
  --out      output file (default target/waveform_export/<file stem>_<start>_<seconds>_<lut>.png)
Ignored harness flags: --nocapture, --quiet, -q";

struct Args {
    audio: PathBuf,
    start: f64,
    seconds: f64,
    lut: Option<String>,
    width: u32,
    height: u32,
    out: Option<PathBuf>,
}

fn parse_args(raw: Vec<String>) -> Result<Args, String> {
    fn number<T: std::str::FromStr>(name: &str, text: &str) -> Result<T, String> {
        text.parse()
            .map_err(|_| format!("cannot parse {text:?} for {name}"))
    }

    let (mut audio, mut seconds, mut lut, mut out) = (None, None, None, None);
    let (mut start, mut width, mut height) = (0.0f64, 1600u32, 300u32);
    let mut raw = raw.into_iter();
    while let Some(name) = raw.next() {
        let mut value = || raw.next().ok_or(format!("missing value for {name}"));
        match name.as_str() {
            "--audio" => audio = Some(PathBuf::from(value()?)),
            "--start" => start = number(&name, &value()?)?,
            "--seconds" => seconds = Some(number::<f64>(&name, &value()?)?),
            "--lut" => lut = Some(value()?),
            "--width" => width = number(&name, &value()?)?,
            "--height" => height = number(&name, &value()?)?,
            "--out" => out = Some(PathBuf::from(value()?)),
            _ => return Err(format!("unknown argument {name}")),
        }
    }

    let audio = audio.ok_or("missing --audio")?;
    let seconds = seconds.ok_or("missing --seconds")?;
    if !(start.is_finite() && start >= 0.0) {
        return Err(format!("--start must be >= 0, got {start}"));
    }
    if !(seconds.is_finite() && seconds > 0.0) {
        return Err(format!("--seconds must be > 0, got {seconds}"));
    }
    if width == 0 || height == 0 {
        return Err("--width and --height must be > 0".to_owned());
    }
    Ok(Args {
        audio,
        start,
        seconds,
        lut,
        width,
        height,
        out,
    })
}

fn pick_lut(requested: Option<&str>) -> Result<Lut, String> {
    let mut luts = discover_luts(&Path::new(env!("CARGO_MANIFEST_DIR")).join("../../LUT"));
    if luts.is_empty() {
        luts.push(Lut::builtin());
    }
    match requested {
        None => Ok(luts.swap_remove(0)),
        Some(name) => {
            let found = luts.iter().position(|l| l.name.eq_ignore_ascii_case(name));
            found.map(|i| luts.swap_remove(i)).ok_or_else(|| {
                let names: Vec<&str> = luts.iter().map(|l| l.name.as_str()).collect();
                format!("unknown --lut {name:?}; available: {}", names.join(", "))
            })
        }
    }
}

fn main() {
    let raw: Vec<String> = std::env::args()
        .skip(1)
        .filter(|a| !matches!(a.as_str(), "--nocapture" | "--quiet" | "-q"))
        .collect();
    if raw.is_empty() {
        println!("{USAGE}");
        return;
    }
    let fail = |message: String| -> ! {
        eprintln!("{message}\n{USAGE}");
        exit(2)
    };
    let args = parse_args(raw).unwrap_or_else(|e| fail(e));
    let lut = pick_lut(args.lut.as_deref()).unwrap_or_else(|e| fail(e));

    let decoded = fretscribe_audio::decode_file(&args.audio).unwrap_or_else(|e| {
        eprintln!("cannot read {}: {e}", args.audio.display());
        exit(1)
    });
    let rate = f64::from(decoded.sample_rate);
    let duration = decoded.mono.len() as f64 / rate;
    if args.start >= duration {
        fail(format!(
            "--start {} is beyond the end of the file ({duration:.3} s)",
            args.start
        ));
    }
    let end = (args.start + args.seconds).min(duration);
    println!(
        "range: {:.3} .. {end:.3} s (file is {duration:.3} s long)",
        args.start
    );

    let from = (args.start * rate) as usize;
    let to = ((end * rate) as usize).clamp(from + 1, decoded.mono.len());
    let slice = &decoded.mono[from..to];
    let pyramid = WaveformPyramid::build(slice, decoded.sample_rate);
    let peaks = pyramid.peaks((0.0, pyramid.duration_sec()), args.width as usize);
    let rgba = render_rgba(&peaks, args.height, &lut);

    let stem = args
        .audio
        .file_stem()
        .map_or_else(|| "audio".into(), |s| s.to_string_lossy().into_owned());
    let path = args.out.unwrap_or_else(|| {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).ancestors().nth(2);
        root.unwrap_or(Path::new("."))
            .join("target/waveform_export")
            .join(format!(
                "{stem}_{}_{}_{}.png",
                args.start, args.seconds, lut.name
            ))
    });
    if let Some(dir) = path.parent().filter(|d| !d.as_os_str().is_empty()) {
        std::fs::create_dir_all(dir)
            .unwrap_or_else(|e| panic!("cannot create {}: {e}", dir.display()));
    }
    image::save_buffer(
        &path,
        &rgba,
        args.width,
        args.height,
        image::ColorType::Rgba8,
    )
    .unwrap_or_else(|e| panic!("cannot write {}: {e}", path.display()));

    let size = image::image_dimensions(&path).expect("PNG is readable");
    assert_eq!(size, (args.width, args.height), "{}", path.display());
    let colours: HashSet<&[u8]> = rgba.chunks_exact(4).collect();
    assert!(colours.len() > 1, "{} is uniform", path.display());

    let absolute = std::path::absolute(&path).unwrap_or(path);
    println!("{}", absolute.display());
    println!("  {} distinct colours, palette {}", colours.len(), lut.name);
}
