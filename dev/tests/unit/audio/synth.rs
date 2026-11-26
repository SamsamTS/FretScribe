use super::*;

const RATE: u32 = 48_000;

fn pitch_by_autocorrelation(x: &[f32], rate: u32, low: f64, high: f64) -> f64 {
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

fn cents(measured: f64, expected: f64) -> f64 {
    1200.0 * (measured / expected).log2()
}

fn note(midi: u8, start_sec: f64, end_sec: f64) -> PlayedNote {
    PlayedNote { id: 0, midi, start_sec, end_sec, velocity: 100, kind: Articulation::Normal, attack: Attack::Pick, pitch_curve: Vec::new() }
}

fn render(notes: &[PlayedNote], preset: SynthPreset, seconds: f64) -> Vec<f32> {
    NoteSynth::render_offline(notes, preset, RATE, PitchRef::default(), seconds)
}

fn window(x: &[f32], from: f64, to: f64) -> &[f32] {
    &x[(from * f64::from(RATE)) as usize..(to * f64::from(RATE)) as usize]
}

fn rms(x: &[f32]) -> f64 {
    (x.iter().map(|s| f64::from(*s).powi(2)).sum::<f64>() / x.len() as f64).sqrt()
}

fn peak(x: &[f32]) -> f32 {
    x.iter().fold(0.0, |m, s| m.max(s.abs()))
}

fn max_step(x: &[f32]) -> f32 {
    x.windows(2).fold(0.0, |m, w| m.max((w[1] - w[0]).abs()))
}

/// Energy of the sample-to-sample difference relative to the signal: grows with brightness.
fn brightness(x: &[f32]) -> f64 {
    let diff: Vec<f32> = x.windows(2).map(|w| w[1] - w[0]).collect();
    rms(&diff) / rms(x)
}

fn amplitude(x: &[f32], freq: f64) -> f64 {
    let (re, im) = x.iter().enumerate().fold((0.0, 0.0), |(re, im), (i, s)| {
        let w = std::f64::consts::TAU * freq * i as f64 / f64::from(RATE);
        (re + f64::from(*s) * w.cos(), im + f64::from(*s) * w.sin())
    });
    2.0 * f64::hypot(re, im) / x.len() as f64
}

fn freq_in(x: &[f32], expected: f64) -> f64 {
    pitch_by_autocorrelation(x, RATE, expected * 0.9, expected * 1.1)
}

fn hz(midi: f32) -> f64 {
    f64::from(midi_to_freq(midi, &PitchRef::default()))
}

#[test]
fn every_preset_is_in_tune_and_comparably_loud() {
    for preset in SynthPreset::all() {
        assert_eq!(SynthPreset::from_name(preset.name()), preset);
        for midi in [40u8, 57, 69, 81] {
            let audio = render(&[note(midi, 0.0, 1.0)], preset, 1.0);
            let expected = hz(f32::from(midi));
            let measured = freq_in(window(&audio, 0.05, 0.2), expected);
            assert!(cents(measured, expected).abs() < 5.0, "{preset:?} {midi} {measured}");
        }
        let chord = render(&[note(40, 0.0, 1.0), note(52, 0.0, 1.0), note(64, 0.0, 1.0)], preset, 0.5);
        assert!(peak(&chord) < 1.0, "{preset:?} {}", peak(&chord));
    }
    let levels: Vec<f64> = SynthPreset::all().iter().map(|&p| rms(window(&render(&[note(52, 0.0, 1.0)], p, 0.5), 0.0, 0.3))).collect();
    let (lo, hi) = levels.iter().fold((f64::MAX, 0.0f64), |m, &l| (m.0.min(l), m.1.max(l)));
    assert!(hi < 2.0 * lo, "{levels:?}");
    assert_eq!(SynthPreset::from_name("?"), SynthPreset::Acoustic);
}

#[test]
fn reference_pitch_applies_to_notes_and_curves() {
    let mut bent = note(57, 0.0, 1.0);
    bent.pitch_curve = vec![(0.1, 0.0), (0.2, 12.0)];
    let audio = NoteSynth::render_offline(&[bent], SynthPreset::Acoustic, RATE, PitchRef::new(432.0).unwrap(), 0.6);
    assert!(cents(freq_in(window(&audio, 0.3, 0.5), 432.0), 432.0).abs() < 5.0);
}

#[test]
fn electric_presets_ring_much_longer_also_high_up() {
    for midi in [45u8, 81] {
        let sustain = |preset| {
            let mut synth = NoteSynth::new(RATE);
            synth.set_preset(preset);
            synth.reset();
            synth.set_performance([note(midi, 0.0, 4.0)]);
            let mut audio = vec![0.0; 3 * RATE as usize];
            // The dry strings: clipping would hide the decay.
            synth.amp.preset = preset;
            synth.render(0.0, 3.0, &mut audio);
            rms(window(&audio, 2.0, 2.5)) / rms(window(&audio, 0.0, 0.5))
        };
        let (acoustic, clean) = (sustain(SynthPreset::Acoustic), sustain(SynthPreset::ElectricClean));
        assert!(clean > 3.0 * acoustic, "{midi} {acoustic} {clean}");
        assert!(sustain(SynthPreset::Distortion) > 3.0 * acoustic);
    }
}

#[test]
fn drive_adds_harmonics_and_zero_drive_does_not_clip() {
    let sine: Vec<f32> = (0..4800).map(|i| 0.8 * (TAU * 300.0 * i as f32 / RATE as f32).sin()).collect();
    let through = |preset| {
        let mut synth = NoteSynth::new(RATE);
        synth.set_preset(preset);
        synth.reset();
        let mut out = sine.clone();
        synth.amp.process(RATE as f32, &mut out);
        out
    };
    let ratio = |x: &[f32]| amplitude(window(x, 0.02, 0.1), 900.0) / amplitude(window(x, 0.02, 0.1), 300.0);
    assert_eq!(through(SynthPreset::Acoustic), sine);
    assert!(ratio(&through(SynthPreset::ElectricClean)) < 0.001);
    assert!(ratio(&through(SynthPreset::Overdrive)) > 0.1);
    assert!(ratio(&through(SynthPreset::Distortion)) > 0.2);
}

#[test]
fn switching_presets_while_playing_neither_clicks_nor_hangs() {
    let mut synth = NoteSynth::new(RATE);
    synth.set_performance([note(45, 0.0, 1.0)]);
    let mut audio = Vec::new();
    let order = [SynthPreset::Acoustic, SynthPreset::Distortion, SynthPreset::Acoustic, SynthPreset::ElectricClean, SynthPreset::Overdrive];
    for i in 0..150 {
        synth.set_preset(order[i / 30]);
        let mut block = [0.0; 480];
        synth.render(i as f64 * 0.01, (i + 1) as f64 * 0.01, &mut block);
        audio.extend_from_slice(&block);
    }
    for switch in 1..order.len() {
        let t = switch as f64 * 0.3;
        if t >= 1.0 {
            break;
        }
        let around = max_step(window(&audio, t - 0.001, t + 0.001));
        let nearby = max_step(window(&audio, t - 0.03, t - 0.001)).max(max_step(window(&audio, t + 0.001, t + 0.03)));
        assert!(around <= nearby * 1.3, "{switch} {around} {nearby}");
    }
    assert!(peak(window(&audio, 1.1, 1.5)) < 1e-5);
    assert!(synth.voices.iter().all(|v| v.note.is_none()));
}

#[test]
fn glides_end_in_tune_upward_and_downward() {
    for preset in SynthPreset::all() {
        for semis in [-12.0f32, -2.0, 2.0, 12.0] {
            let mut n = note(57, 0.0, 1.0);
            n.pitch_curve = vec![(0.1, 0.0), (0.25, semis)];
            let audio = render(&[n], preset, 0.6);
            let (from, to) = (hz(57.0), hz(57.0 + semis));
            assert!(cents(freq_in(window(&audio, 0.02, 0.1), from), from).abs() < 6.0, "{preset:?} {semis}");
            let measured = freq_in(window(&audio, 0.3, 0.45), to);
            assert!(cents(measured, to).abs() < 6.0, "{preset:?} {semis} {measured} {to}");
        }
    }
}

#[test]
fn bends_and_vibrato_are_continuous() {
    let steady = render(&[note(64, 0.0, 1.0)], SynthPreset::ElectricClean, 0.8);
    let mut n = note(64, 0.0, 1.0);
    n.pitch_curve = (0..80).map(|i| {
        let t = 0.1 + 0.01 * f64::from(i);
        (t, if i < 20 { i as f32 * 0.1 } else { 2.0 + 0.35 * (t * 5.5 * std::f64::consts::TAU).sin() as f32 })
    }).collect();
    let moving = render(&[n], SynthPreset::ElectricClean, 0.8);
    assert!(max_step(window(&moving, 0.1, 0.8)) < 1.5 * max_step(window(&steady, 0.1, 0.8)));
    assert!(rms(window(&moving, 0.6, 0.8)) > 0.3 * rms(window(&steady, 0.6, 0.8)));
}

#[test]
fn kinds_and_attacks_change_the_sound() {
    let with = |kind, attack| {
        let mut n = note(57, 0.0, 1.0);
        (n.kind, n.attack) = (kind, attack);
        render(&[n], SynthPreset::Acoustic, 1.0)
    };
    let normal = with(Articulation::Normal, Attack::Pick);
    let harmonic = with(Articulation::Harmonic, Attack::Pick);
    assert!(brightness(window(&harmonic, 0.0, 0.1)) > 1.2 * brightness(window(&normal, 0.0, 0.1)));
    let muted = with(Articulation::Muted, Attack::Pick);
    assert!(rms(window(&muted, 0.3, 0.4)) < 0.3 * rms(window(&normal, 0.3, 0.4)));
    let soft = with(Articulation::Normal, Attack::Soft);
    assert!(rms(window(&soft, 0.0, 0.2)) < 0.9 * rms(window(&normal, 0.0, 0.2)));

    let dead = with(Articulation::Dead, Attack::Pick);
    assert!(rms(window(&dead, 0.0, 0.02)) > 0.01);
    assert!(rms(window(&dead, 0.15, 0.2)) < 0.01 * rms(window(&dead, 0.0, 0.02)));
    // Unpitched: no periodicity at the note's period.
    let x = window(&dead, 0.0, 0.04);
    let lag = (f64::from(RATE) / hz(57.0)).round() as usize;
    let corr: f64 = x.iter().zip(&x[lag..]).map(|(a, b)| f64::from(*a) * f64::from(*b)).sum();
    assert!(corr.abs() / (rms(x).powi(2) * x.len() as f64) < 0.25);
}

#[test]
fn plucks_start_on_time_and_stop_after_release() {
    for preset in SynthPreset::all() {
        let audio = render(&[note(60, 0.5, 0.75)], preset, 1.5);
        let first = audio.iter().position(|s| s.abs() > 1e-4).unwrap();
        assert!((first as i64 - 24_000).abs() <= 64, "{first}");
        assert!(peak(window(&audio, 0.8, 1.5)) < 1e-5);
    }
}

#[test]
fn invalid_notes_are_dropped_and_overlaps_are_polyphonic() {
    let mut synth = NoteSynth::new(RATE);
    synth.set_performance([note(200, 0.0, 1.0), note(60, f64::NAN, 1.0), note(60, 0.2, 0.1), note(64, 0.0, 1.0)]);
    assert_eq!(synth.notes.len(), 2);
    let chord = render(&[note(60, 0.0, 1.0), note(64, 0.0, 1.0), note(67, 0.0, 1.0)], SynthPreset::Acoustic, 0.5);
    let single = render(&[note(60, 0.0, 1.0)], SynthPreset::Acoustic, 0.5);
    assert!(rms(&chord) > 1.4 * rms(&single));
}

#[test]
fn many_simultaneous_notes_steal_the_oldest_voice() {
    let notes: Vec<PlayedNote> = (0..40).map(|i| note(40 + i, 0.0, 1.0)).collect();
    let audio = render(&notes, SynthPreset::Acoustic, 0.2);
    assert!(peak(&audio) > 0.01);
    assert!(audio.iter().all(|s| s.is_finite()));
}

#[test]
fn seek_skips_earlier_notes_and_reset_leaves_no_tail() {
    let mut synth = NoteSynth::new(RATE);
    synth.set_preset(SynthPreset::Distortion);
    synth.set_performance([note(60, 0.1, 1.0), note(62, 0.5, 1.0)]);
    synth.seek(0.3);
    let mut out = vec![0.0; 64];
    synth.render(0.0, 0.0013, &mut out);
    assert!(out.iter().all(|s| *s == 0.0));
    synth.render(0.5, 0.5013, &mut out);
    assert!(out.iter().any(|s| *s != 0.0));
    synth.reset();
    out.fill(0.0);
    synth.render(0.6, 0.6013, &mut out);
    assert!(out.iter().all(|s| *s == 0.0));
}
