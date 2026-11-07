//! Prints the offset candidates of a track relative to a reference track.
//!
//! cargo run --release -p fretscribe-analysis --example sync_offset -- ref.wav other.wav [--max-lag 15]

use std::process::exit;

use fretscribe_analysis::sync::{
    assume_synced, estimate_offset, SyncOptions, DEFAULT_TOLERANCE_MS,
};

const USAGE: &str = "usage: sync_offset <reference> <other> [--max-lag sec]";

fn fail(message: &str) -> ! {
    eprintln!("{message}\n{USAGE}");
    exit(2)
}

fn main() {
    let (mut files, mut opts) = (Vec::new(), SyncOptions::default());
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--max-lag" => {
                opts.max_lag_sec = args
                    .next()
                    .and_then(|v| v.parse().ok())
                    .unwrap_or_else(|| fail("invalid --max-lag"));
            }
            _ if !arg.starts_with("--") => files.push(arg),
            _ => fail(&format!("unknown argument {arg}")),
        }
    }
    let [reference, other] = files.as_slice() else { fail("expected two audio files") };
    let load = |path: &String| {
        fretscribe_audio::decode_file(path).unwrap_or_else(|e| {
            eprintln!("{path}: {e}");
            exit(1)
        })
    };
    let (reference, other) = (load(reference), load(other));
    if reference.sample_rate != other.sample_rate {
        fail("the files have different sample rates; resample one first");
    }
    println!("durations: {:.3} s / {:.3} s", reference.duration_sec, other.duration_sec);
    if assume_synced(reference.duration_sec, other.duration_sec, DEFAULT_TOLERANCE_MS) {
        println!("same length: assumed in sync (offset 0)");
        return;
    }
    let candidates = estimate_offset(&reference.mono, &other.mono, reference.sample_rate, opts);
    for (i, c) in candidates.iter().enumerate() {
        println!(
            "#{} offset {:+.4} s  confidence {:.2}  {}",
            i + 1,
            c.offset_sec,
            c.confidence,
            c.note.as_deref().unwrap_or("")
        );
    }
    if candidates.is_empty() {
        println!("no candidates (signals too short or silent)");
    }
}
