//! Exports an excerpt from the middle of a track as PNG(s) for visual inspection, and checks the
//! render pipeline. `harness = false`: run it with `cargo test -p fretscribe-analysis --test
//! export_png -- --audio song.flac --lut Heat --black 0.25 --white 0.9 --gamma 0.7,1.0,1.5`.

use std::collections::HashSet;
use std::f32::consts::TAU;
use std::path::{Path, PathBuf};
use std::process::exit;

use fretscribe_analysis::{
    compute_spectrogram, discover_luts, Levels, Lut, Spectrogram, SpectrogramParams,
};
use fretscribe_core::{freq_to_midi_float, PitchRef};

const WIDTH: usize = 1600;
const HEIGHT: usize = 640;
const SYNTH_RATE: u32 = 44_100;
const SYNTH_SECONDS: f32 = 20.0;
const USAGE: &str = "\
usage: export_png [--audio <path>] [--seconds <f>] [--lut <name>] [--black <f>] [--white <f>]
                  [--gamma <v[,v,...]>] [--out <dir>]
  --audio    audio file (default: synthetic 20 s chord)
  --seconds  excerpt length, centred on the middle of the track (default 5.0)
  --lut      palette name from the LUT folder (default: the first one)
  --black    black point, 0..1 (default 0.0)
  --white    white point, 0..1 (default 1.0)
  --gamma    one value or a comma-separated list, one PNG each (default 1.0)
  --out      output folder (default target/spectrogram_export/)
Ignored harness flags: --nocapture, --quiet, -q";

struct Args {
    audio: Option<PathBuf>,
    seconds: f64,
    lut: Option<String>,
    black: f32,
    white: f32,
    gammas: Vec<f32>,
    out: PathBuf,
}

fn parse_args() -> Result<Args, String> {
    let mut args = Args {
        audio: None,
        seconds: 5.0,
        lut: None,
        black: 0.0,
        white: 1.0,
        gammas: vec![1.0],
        out: Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/spectrogram_export"),
    };
    let mut raw = std::env::args().skip(1);
    while let Some(name) = raw.next() {
        if matches!(name.as_str(), "--nocapture" | "--quiet" | "-q") {
            continue;
        }
        let mut value = |name: &str| raw.next().ok_or(format!("missing value for {name}"));
        fn number<T: std::str::FromStr>(name: &str, text: &str) -> Result<T, String> {
            text.parse()
                .map_err(|_| format!("cannot parse {text:?} for {name}"))
        }
        match name.as_str() {
            "--audio" => args.audio = Some(PathBuf::from(value(&name)?)),
            "--seconds" => args.seconds = number(&name, &value(&name)?)?,
            "--lut" => args.lut = Some(value(&name)?),
            "--black" => args.black = number(&name, &value(&name)?)?,
            "--white" => args.white = number(&name, &value(&name)?)?,
            "--gamma" => {
                args.gammas = value(&name)?
                    .split(',')
                    .map(|g| number(&name, g.trim()))
                    .collect::<Result<_, _>>()?;
            }
            "--out" => args.out = PathBuf::from(value(&name)?),
            _ => return Err(format!("unknown argument {name}")),
        }
    }
    if !(args.seconds.is_finite() && args.seconds > 0.0) {
        return Err(format!("--seconds must be positive, got {}", args.seconds));
    }
    Ok(args)
}

const CHORD_HZ: [(f32, f32); 5] = [
    (110.0, 0.12),
    (164.81, 0.12),
    (220.0, 0.3),
    (261.63, 0.12),
    (329.63, 0.12),
];
const HARMONICS: usize = 5;
const PLUCK_PERIOD_SEC: f32 = 2.5;
const LOUDEST_HZ: f32 = 220.0;

fn synthetic_chord() -> Vec<f32> {
    let rate = SYNTH_RATE as f32;
    let mut signal: Vec<f32> = (0..(SYNTH_SECONDS * rate) as usize)
        .map(|i| {
            let t = i as f32 / rate;
            let since_pluck = t % PLUCK_PERIOD_SEC;
            let decay = (-since_pluck / 1.2).exp();
            let strum: f32 = CHORD_HZ
                .iter()
                .map(|&(hz, amp)| {
                    (1..=HARMONICS)
                        .map(|h| amp / h as f32 * (TAU * hz * h as f32 * since_pluck).sin())
                        .sum::<f32>()
                })
                .sum();
            strum * decay
        })
        .collect();
    let peak = signal.iter().fold(0.0f32, |m, v| m.max(v.abs()));
    signal.iter_mut().for_each(|v| *v *= 0.9 / peak.max(1e-6));
    signal
}

fn grey_lut() -> Lut {
    let mut lut = Lut::builtin();
    lut.name = "Grey".into();
    for (i, color) in lut.colors.iter_mut().enumerate() {
        *color = [i as u8; 3]
            .into_iter()
            .chain([255])
            .collect::<Vec<_>>()
            .try_into()
            .unwrap_or_default();
    }
    lut
}

fn load_signal(audio: &Option<PathBuf>) -> (Vec<f32>, u32, String) {
    match audio {
        None => (synthetic_chord(), SYNTH_RATE, "synthetic".to_owned()),
        Some(path) => {
            let decoded = fretscribe_audio::decode_file(path)
                .unwrap_or_else(|e| panic!("cannot read {}: {e}", path.display()));
            let stem = path
                .file_stem()
                .map_or_else(|| "audio".into(), |s| s.to_string_lossy().into_owned());
            (decoded.mono, decoded.sample_rate, stem)
        }
    }
}

fn render(
    spec: &Spectrogram,
    range: (f64, f64),
    params: &SpectrogramParams,
    lut: &Lut,
    levels: Levels,
    gamma: f32,
) -> Vec<u8> {
    spec.render_rgba(
        range,
        WIDTH,
        HEIGHT,
        (params.min_midi, params.max_midi),
        &PitchRef::default(),
        lut,
        levels,
        gamma,
    )
}

fn main() {
    let args = parse_args().unwrap_or_else(|e| {
        eprintln!("{e}\n{USAGE}");
        exit(2)
    });

    let mut luts = discover_luts(&Path::new(env!("CARGO_MANIFEST_DIR")).join("../../LUT"));
    if luts.is_empty() {
        luts.push(Lut::builtin());
    }
    let lut = match &args.lut {
        None => luts[0].clone(),
        Some(name) => luts
            .iter()
            .find(|l| &l.name == name)
            .cloned()
            .unwrap_or_else(|| {
                let names: Vec<&str> = luts.iter().map(|l| l.name.as_str()).collect();
                eprintln!(
                    "unknown palette {name:?}; available: {}\n{USAGE}",
                    names.join(", ")
                );
                exit(2)
            }),
    };
    let levels = Levels::new(args.black, args.white);

    let (mono, rate, stem) = load_signal(&args.audio);
    let duration = mono.len() as f64 / f64::from(rate);
    let seconds = args.seconds.min(duration);
    let start = ((duration - seconds) / 2.0).max(0.0);

    let params = SpectrogramParams::default();
    let margin = f64::from(params.max_window_sec) / 2.0;
    let slice_start = (start - margin).max(0.0);
    let slice_end = (start + seconds + margin).min(duration);
    let from = (slice_start * f64::from(rate)) as usize;
    let to = ((slice_end * f64::from(rate)) as usize).min(mono.len());
    let spec = compute_spectrogram(&mono[from..to], rate, &params);
    let data_before = spec.data.clone();
    let range = (start - slice_start, start - slice_start + seconds);

    std::fs::create_dir_all(&args.out)
        .unwrap_or_else(|e| panic!("cannot create {}: {e}", args.out.display()));
    let mut renders = Vec::new();
    for &gamma in &args.gammas {
        let rgba = render(&spec, range, &params, &lut, levels, gamma);
        let path = args.out.join(format!(
            "{stem}_{}_b{}_w{}_g{gamma}.png",
            lut.name, levels.black, levels.white
        ));
        image::save_buffer(
            &path,
            &rgba,
            WIDTH as u32,
            HEIGHT as u32,
            image::ColorType::Rgba8,
        )
        .unwrap_or_else(|e| panic!("cannot write {}: {e}", path.display()));
        println!("{}", path.display());

        let (w, h) = image::image_dimensions(&path).expect("PNG is readable");
        assert_eq!(
            (w as usize, h as usize),
            (WIDTH, HEIGHT),
            "{}",
            path.display()
        );
        let colours: HashSet<[u8; 4]> = rgba
            .chunks_exact(4)
            .map(|p| [p[0], p[1], p[2], p[3]])
            .collect();
        assert!(colours.len() > 1, "{} is uniform", path.display());
        println!("  {} distinct colours", colours.len());
        renders.push(rgba);
    }

    let differs = match renders.as_slice() {
        [first, second, ..] => first != second,
        _ => {
            let g = args.gammas[0];
            render(&spec, range, &params, &lut, levels, g * 0.5)
                != render(&spec, range, &params, &lut, levels, g * 2.0)
                && render(&spec, range, &params, &lut, Levels::default(), g)
                    != render(&spec, range, &params, &lut, Levels::new(0.5, 1.0), g)
        }
    };
    assert!(
        differs,
        "different gammas or levels must give different images"
    );
    assert_eq!(
        spec.data, data_before,
        "rendering must not touch the spectrogram data"
    );

    if args.audio.is_none() {
        let image = render(&spec, range, &params, &grey_lut(), Levels::default(), 1.0);
        let brightness = |y: usize| -> u64 {
            image[y * WIDTH * 4..][..WIDTH * 4]
                .chunks_exact(4)
                .map(|p| u64::from(p[0]))
                .sum()
        };
        let brightest = (0..HEIGHT)
            .max_by_key(|&y| brightness(y))
            .expect("image has rows");
        let per_pixel = (params.max_midi - params.min_midi) / HEIGHT as f32;
        let midi = params.max_midi - (brightest as f32 + 0.5) * per_pixel;
        let expected = freq_to_midi_float(LOUDEST_HZ, &PitchRef::default());
        println!("brightest row: MIDI {midi:.2} (expected {expected:.2})");
        assert!(
            (midi - expected).abs() < 0.25,
            "brightest row is MIDI {midi}, expected {expected}"
        );
    }
}
