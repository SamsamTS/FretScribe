# Contract changes

## 2026-11-01 - audio crate

Workspace dependencies (root `Cargo.toml`):
- Added `signalsmith-stretch = "0.1"` (MIT) for the pitch-preserving time-stretch. It compiles a bundled C++ library in `build.rs` and runs `bindgen`, so building needs a C++ toolchain and libclang (`LIBCLANG_PATH` on Windows). It is used only through `crates/audio/src/stretch.rs`.
- `rodio` now uses `default-features = false`: decoding is done by symphonia in `fretscribe-audio`, rodio is only the output layer.
- `serde_json` now enables `float_roundtrip`. The default parser can be off by one ULP, so a project saved and reloaded was not equal to itself (`core` test `edit::tests::json_round_trip_after_edits` failed on `start_sec` values). No source change in `core`.

New public API consumed by other crates (`fretscribe-audio`), all in project time:
- `decode_file`, `DecodedAudio`, `AudioError`.
- `Player`: `new`, `set_tracks(Vec<TrackAudio>)`, `load`, `play`, `pause`, `stop`, `seek`, `is_playing`, `position_sec`, `duration_sec`, `set_volume`, `set_region`, `set_loop`, `set_playback_speed`, `set_speed_mode(SpeedMode)`, `set_track_{volume,mute,solo,offset}`, `track_peak_levels`, `set_note_overlay`, `set_reference_pitch`.
- Track audibility follows `core::audible_tracks` and volume is clamped like `Track::set_volume` (`0..=Track::MAX_VOLUME`).

## 2026-11-01 - audio crate: faster, non-blocking loading

- Root `Cargo.toml`: `[profile.dev.package."*"] opt-level = 3` optimises third-party dependencies (symphonia, rubato, signalsmith-stretch) in debug builds; workspace crates stay at opt-level 0. Optional, not applied: `[profile.dev.package.fretscribe-audio] opt-level = 1` cuts debug load time of the preparation step roughly five-fold.
- New in `fretscribe-audio`, all backward compatible:
  - `PreparedAudio::new(&DecodedAudio, output_rate)`: the expensive resampling step, `Send + Sync`, cheap to clone. Use `Player::output_rate()` for the rate.
  - `PreparedTrack` and `Player::set_prepared_tracks(Vec<PreparedTrack>)`: cheap swap-in (a few ms for 380 s of audio); fails with `AudioError::RateMismatch` if a track was prepared for another rate.
  - `decode_files(&[path])`: parallel decoding, results in input order, one `Result` per file.
- `Player::set_tracks` and `load` keep their signatures and behaviour. They now prepare missing tracks in parallel and no longer hold the internal state lock while resampling, so `duration_sec()` stays callable from the UI thread during a load.
- Preparation streams the resample straight into the final interleaved buffer (no full-length per-channel copies); output is numerically identical to the previous implementation (tested to 1e-6).

## 2026-11-01 - audio crate: guitar focus filter

A listening aid for full-mix recordings. The UI should expose only a toggle button and a strength slider; everything else below is project context the app passes in.

- New public API in `fretscribe-audio`, all backward compatible (no existing signature or `TrackAudio` field changed):
  - `Player::set_guitar_focus(bool)` and `Player::set_guitar_focus_strength(f32)` (clamped to 0..1, non-finite ignored; default strength 0.5). Default is OFF.
  - `Player::set_focus_context(&Tuning, capo: u8)`. The reference pitch comes from the existing `Player::set_reference_pitch`, which now also retunes the focus band. Defaults: standard tuning, capo 0, A4 = 440 Hz. The app should call it on project open and whenever the tuning or capo changes (including undo/redo).
  - `FocusBand { low_hz, high_hz }` and `focus_band(&Tuning, capo, &PitchRef)`: the span of fundamentals the guitar can play (lowest open string + capo, to the highest string + capo + 24 frets), derived with `core::midi_to_freq`, never a hardcoded 440. At A4 = 440 Hz in standard tuning: 82.41 Hz to 1318.5 Hz.
  - `apply_focus_offline(&[f32], rate, &FocusBand, strength)`: zero-phase (forward-backward) version for the analysis crate, so onset times do not move.
- Behaviour: the filter acts on the SUM of the audio tracks, before the note-synth overlay is added, master volume and the soft limiter, so the overlay is never filtered. It is zero-latency (position accuracy is unaffected). Off, or strength 0, is bit-exact identical to the unfiltered output. Toggling, strength, context and speed changes are click-free (10 ms ramps and crossfades). In ChangePitch mode the band follows the playback speed; in PreservePitch it does not.
- Design: 4th-order Butterworth high-pass (corner 0.6x the lowest fundamental) and low-pass (corner 4x the highest fundamental, capped below Nyquist), each blended with the dry signal, giving a flat cut of up to 24 dB at strength 1. Measured through the engine at strength 1, standard tuning at 440 Hz: E2 -0.68 dB, 440 Hz -0.03 dB, E6 -0.10 dB, 3x E6 -1.24 dB, one octave below the low corner -20.5 dB, one octave above the high corner -21.7 dB. RBJ shelving filters were tried first and rejected: at the same corner they cost the lowest open string 4.8 dB.
- Internal: `Ramp` moved from `engine.rs` to its own module `ramp.rs` (no behaviour change).
- Not exposed to the user and not applied by the app shell to spectrograms or analysis: the filter only changes what is heard, like mute and solo. No change outside `crates/audio`.

## 2026-11-02 - analysis crate: spectrogram

Workspace (root `Cargo.toml`): added `rayon = "1"` and `image = { version = "0.25", default-features = false, features = ["png"] }`; `[profile.dev.package.fretscribe-analysis] opt-level = 3` so the transform is usable in debug builds. No change in `core` or `audio`.

New public API in `fretscribe-analysis` (all in project time, seconds):
- `compute_spectrogram(&[f32], sample_rate, &SpectrogramParams) -> Spectrogram` and `compute_spectrogram_streaming(Arc<[f32]>, sample_rate, &params) -> SpectrogramStream { info, chunks }` (chunks arrive in time order; apply with `Spectrogram::empty(&info)` + `apply_chunk`; dropping the receiver cancels).
- `Spectrogram { data: Vec<f32> 0..1, n_frames, n_rows, hop_sec, min_midi, max_midi, bins_per_semitone }`, row-major, row 0 = lowest pitch, frame `i` centred at `i * hop_sec`. Row `r` is centred on grid pitch `min_midi + r / bins_per_semitone` relative to A4 = 440 Hz, whatever the project's reference pitch. The grid is never recomputed when the reference changes: `row_to_midi` / `midi_to_row` (methods, they need the grid) take the `PitchRef`.
- `render_rgba(time_range_sec, w, h, midi_range, &PitchRef, &Lut, Levels, gamma)`: average downsampling when zoomed out, bilinear when zoomed in; image row 0 = highest pitch.
- `Levels` (`new`, `clamped`, `to_db`, `MIN_LEVELS_GAP`), `apply_levels`, `apply_gamma`, `clamp_gamma`, `GAMMA_MIN/MAX/DEFAULT`; `analysis::lut::{Lut, discover_luts}`; `AnalysisError`.
- Palettes are PNGs in the repository-root `LUT/` folder (`Classic.png`, `Heat.png`, regenerated by `cargo run -p fretscribe-analysis --example make_luts`). The folder must ship next to the executable; `Lut::builtin()` is only the fallback.

Design notes: half-octave bands, each with its own FFT size and a Hann window of `q / f_lowest_of_band` seconds (clamped), so every row has at least the resolution `q` promises (at most 1.4x less time resolution at the top of a band). Long-window bands are evaluated every `window / 8` frames and interpolated in time. A 300 s file takes about 1.8 s in release on 2 cores.

`cargo test -p fretscribe-analysis --test export_spectrogram_png -- <args>` is a plain `fn main()` (`harness = false`); it also ignores `--nocapture`, `--quiet` and `-q` so `cargo test --workspace -- --nocapture` keeps working.

## 2026-11-02 - analysis crate: application display defaults

- `APP_GAMMA = 0.8` and `APP_WHITE = 0.8` (black 0.0): the starting display values chosen by eye. The identity defaults (`Levels::default()`, `GAMMA_DEFAULT = 1.0`) are unchanged.
- `Spectrogram::suggested_levels()` returns black 0.0 and white at the strongest value of the data, for when a track is first loaded (falls back to `APP_WHITE` for silent or empty data). The user can then adjust with the slider.

## 2026-11-03 - analysis crate: waveform peaks

No workspace dependency change (`rayon` and `image` were already in `fretscribe-analysis`). The `[[test]] export_waveform_png` target is declared with `harness = false`.

New public API in `fretscribe-analysis` (module `waveform`, also re-exported at the crate root):
- `WaveformPyramid::build(&[f32], sample_rate)`: min/max/sum-of-squares blocks, base block `BASE_BLOCK = 64` samples, each level merging pairs of the previous one; built in parallel with rayon. Accessors `sample_rate()` and `duration_sec()`.
- `WaveformPyramid::peaks((start_sec, end_sec), pixel_width) -> Vec<(min, max, rms)>`: exactly `pixel_width` columns, from the coarsest level whose blocks fit in a pixel. Zero width gives an empty vector; invalid or empty ranges, and the parts of a range outside the audio, give silent `(0, 0, 0)` columns. The finest resolution is one base block (no raw samples are kept), so zooming in beyond 64 samples per pixel repeats blocks instead of leaving gaps. Non-finite samples count as 0.
- `waveform::render_rgba(&[(min, max, rms)], height, &Lut) -> Vec<u8>` (crate-root alias `render_waveform_rgba`): one column per peak, row 0 on top, background = `lut.colors[0]`, a centre line, the min..max envelope coloured by the column's loudness and a brighter RMS body. Amplitude goes through `apply_gamma` with the module constant `waveform::WAVEFORM_GAMMA` (1.0), which is deliberately not a parameter or setting. The colour level is smoothed along the time axis by a moving average of `waveform::WAVEFORM_COLOR_SMOOTHING_PX` pixels (15; 0 or 1 disables it; window `2 * (n / 2) + 1`), also a hand-tuned constant. Both the envelope and the RMS body also fade vertically, most intense on the centre line and `EDGE_SCALE` (0.4) times that at the tips (private constants `EDGE_SCALE`, `BODY_LIFT`, `ENVELOPE_SHARE` in `waveform.rs`); the body stays brighter than the envelope even at full scale. Only colours are affected, the envelope and body outlines are unchanged, and silent columns are left out of the average so notes do not fade at their edges.
- `Lut::color(t) -> [u8; 4]` (in `lut.rs`): lookup for `t` in 0..=1, clamped, NaN gives the weakest colour. Same index maths as the spectrogram renderer. The spectrogram module and its `Lut` already existed, so no LUT stub was added.

Tests and tooling: unit tests in `waveform.rs` and `lut.rs`; `cargo test -p fretscribe-analysis --test export_waveform_png -- --audio <file> --seconds <n> [--start <s>] [--lut Classic|Heat] [--width <px>] [--height <px>] [--out <png>]` writes a PNG (prints the usage and succeeds without arguments); `cargo run --release -p fretscribe-analysis --example waveform_peaks -- <file>` prints peaks.

## 2026-11-03 - ui crate: measure grid (core additions)

Backward compatible; no existing signature or behaviour changed.
- `Propagation { Following (default), Single }` and `set_tempo_scoped` / `set_time_sig_scoped`, both as functions in `measures` and as `Project` methods (`Project::set_tempo_scoped(index, bpm, propagation, policy)`, `Project::set_time_sig_scoped(...)`). `set_tempo` / `set_time_sig` are now thin wrappers using `Following`, so they behave exactly as before. `Single` edits only `index`, which backs the "apply to following identical measures" checkbox of the grid popup.
- `UndoStack` (`undo` module, also re-exported with `DEFAULT_UNDO_LIMIT = 200`): linear history of `ProjectEdit`. `push`, `undo(&mut Project) -> Option<&ProjectEdit>`, `redo(...)`, `can_undo`, `can_redo`, `undo_label`, `redo_label`, `clear`, `with_limit`. Pushing discards the redo branch and the oldest edits are dropped beyond the limit. It is not generic: every edit already produces a `ProjectEdit` snapshot pair, so `UndoStack<ProjectEdit>` would be the only instantiation.

## 2026-11-05 - ui crate: overview timeline always snaps

- `OverviewTimeline::snap_region` is removed and Ctrl no longer toggles snapping: the seek point (click and drag) and the region edges (create, resize, move) always snap to `snap_points` (default: `measure_starts`); the viewport box does not. No points, no snapping.
- `DragContext::snap` is now `&[f64]` instead of `Option<&[f64]>`; `DragUpdate::Seek` is snapped.
