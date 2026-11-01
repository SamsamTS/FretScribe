use std::f64::consts::TAU;
use std::sync::Arc;

use crate::decode::DecodedAudio;
use crate::engine::{Engine, BLOCK_FRAMES};
use crate::player::{Player, TrackAudio};

pub const RATE: u32 = 48_000;

pub fn sine(freq: f64, rate: u32, seconds: f64, amp: f32) -> Vec<f32> {
    let frames = (seconds * f64::from(rate)).round() as usize;
    (0..frames)
        .map(|i| amp * (TAU * freq * i as f64 / f64::from(rate)).sin() as f32)
        .collect()
}

pub fn mono_audio(rate: u32, samples: Vec<f32>) -> Arc<DecodedAudio> {
    Arc::new(DecodedAudio::from_interleaved(rate, 1, samples).unwrap())
}

pub fn track(id: u64, samples: Vec<f32>) -> TrackAudio {
    TrackAudio {
        id,
        audio: mono_audio(RATE, samples),
        offset_sec: 0.0,
        volume: 1.0,
        muted: false,
        solo: false,
    }
}

pub fn dc(seconds: f64, value: f32) -> Vec<f32> {
    vec![value; (seconds * f64::from(RATE)) as usize]
}

pub fn setup(tracks: Vec<TrackAudio>) -> (Player, Engine) {
    let (player, engine) = Player::headless(RATE);
    player.set_tracks(tracks).unwrap();
    (player, engine)
}

pub fn blocks(seconds: f64) -> usize {
    (seconds * f64::from(RATE) / BLOCK_FRAMES as f64).round() as usize
}

pub struct Rendered {
    pub left: Vec<f32>,
    pub positions: Vec<f64>,
}

pub fn render(player: &Player, engine: &mut Engine, blocks: usize) -> Rendered {
    let mut left = Vec::with_capacity(blocks * BLOCK_FRAMES);
    let mut positions = Vec::with_capacity(blocks);
    let mut buffer = vec![0.0; 2 * BLOCK_FRAMES];
    for _ in 0..blocks {
        engine.render_block(&mut buffer);
        left.extend(buffer.chunks_exact(2).map(|f| f[0]));
        positions.push(player.position_sec());
    }
    Rendered { left, positions }
}

pub fn render_sec(player: &Player, engine: &mut Engine, seconds: f64) -> Rendered {
    render(player, engine, blocks(seconds))
}

pub fn max_step(x: &[f32]) -> f32 {
    x.windows(2).map(|w| (w[1] - w[0]).abs()).fold(0.0, f32::max)
}

pub fn rms(x: &[f32]) -> f32 {
    (x.iter().map(|v| v * v).sum::<f32>() / x.len() as f32).sqrt()
}

pub fn pitch_by_autocorrelation(x: &[f32], rate: u32, low: f64, high: f64) -> f64 {
    let lag_min = (f64::from(rate) / high) as usize;
    let lag_max = (f64::from(rate) / low) as usize;
    let corr = |lag: usize| -> f64 {
        x.iter().zip(&x[lag..]).map(|(a, b)| f64::from(*a) * f64::from(*b)).sum()
    };
    let best = (lag_min..=lag_max)
        .max_by(|&a, &b| corr(a).total_cmp(&corr(b)))
        .unwrap();
    let (y0, y1, y2) = (corr(best - 1), corr(best), corr(best + 1));
    let shift = 0.5 * (y0 - y2) / (y0 - 2.0 * y1 + y2);
    f64::from(rate) / (best as f64 + shift)
}

pub fn cents(measured: f64, expected: f64) -> f64 {
    1200.0 * (measured / expected).log2()
}
