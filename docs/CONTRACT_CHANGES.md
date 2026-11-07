# Contract changes

## 2026-11-01 - audio crate

Workspace dependencies (root `Cargo.toml`):
- Added `signalsmith-stretch = "0.1"` (MIT) for the pitch-preserving time-stretch. It compiles a bundled C++ library in `build.rs` and runs `bindgen`, so building needs a C++ toolchain and libclang (`LIBCLANG_PATH` on Windows). It is used only through `crates/audio/src/stretch.rs`.
- `rodio` now uses `default-features = false`: decoding is done by symphonia in `fretscribe-audio`, rodio is only the output layer.
- `serde_json` now enables `float_roundtrip`. The default parser can be off by one ULP, so a saved and reloaded project would not equal itself.

New public API consumed by other crates (`fretscribe-audio`), all in project time:
- `decode_file`, `DecodedAudio`, `AudioError`.
- `Player`: `new`, `set_tracks(Vec<TrackAudio>)`, `load`, `play`, `pause`, `stop`, `seek`, `is_playing`, `position_sec`, `duration_sec`, `set_volume`, `set_region`, `set_loop`, `set_playback_speed`, `set_speed_mode(SpeedMode)`, `set_track_{volume,mute,solo,offset}`, `track_peak_levels`, `set_note_overlay`, `set_reference_pitch`.
- `PreparedAudio::new(&DecodedAudio, output_rate)`: the expensive resampling step, `Send + Sync`, cheap to clone. Use `Player::output_rate()` for the rate. Preparation streams the resample straight into the final interleaved buffer.
- `PreparedTrack` and `Player::set_prepared_tracks(Vec<PreparedTrack>)`: cheap swap-in (a few ms for 380 s of audio); fails with `AudioError::RateMismatch` if a track was prepared for another rate.
- `decode_files(&[path])`: parallel decoding, results in input order, one `Result` per file.
- `Player::set_tracks` and `load` prepare missing tracks in parallel and do not hold the internal state lock while resampling, so `duration_sec()` stays callable from the UI thread during a load.
- Root `Cargo.toml`: `[profile.dev.package."*"] opt-level = 3` optimises third-party dependencies (symphonia, rubato, signalsmith-stretch) in debug builds; workspace crates stay at opt-level 0. Optional, not applied: `[profile.dev.package.fretscribe-audio] opt-level = 1` cuts debug load time of the preparation step roughly five-fold.
- Track audibility follows `core::audible_tracks` and volume is clamped like `Track::set_volume` (`0..=Track::MAX_VOLUME`).

## 2026-11-01 - audio crate: guitar focus filter

A listening aid for full-mix recordings. The UI should expose only a toggle button and a strength slider; everything else below is project context the app passes in.

- New public API in `fretscribe-audio`, all backward compatible (no existing signature or `TrackAudio` field changed):
  - `Player::set_guitar_focus(bool)` and `Player::set_guitar_focus_strength(f32)` (clamped to 0..1, non-finite ignored; default strength 0.5). Default is OFF.
  - `Player::set_focus_context(&Tuning, capo: u8)`. The reference pitch comes from the existing `Player::set_reference_pitch`, which now also retunes the focus band. Defaults: standard tuning, capo 0, A4 = 440 Hz. The app should call it on project open and whenever the tuning or capo changes (including undo/redo).
  - `FocusBand { low_hz, high_hz }` and `focus_band(&Tuning, capo, &PitchRef)`: the span of fundamentals the guitar can play (lowest open string + capo, to the highest string + capo + 24 frets), derived with `core::midi_to_freq`, never a hardcoded 440. At A4 = 440 Hz in standard tuning: 82.41 Hz to 1318.5 Hz.
  - `apply_focus_offline(&[f32], rate, &FocusBand, strength)`: zero-phase (forward-backward) version for the analysis crate, so onset times do not move.
- Behaviour: the filter acts on the SUM of the audio tracks, before the note-synth overlay is added, master volume and the soft limiter, so the overlay is never filtered. It is zero-latency (position accuracy is unaffected). Off, or strength 0, is bit-exact identical to the unfiltered output. Toggling, strength, context and speed changes are click-free (10 ms ramps and crossfades). In ChangePitch mode the band follows the playback speed; in PreservePitch it does not.
- Design: 4th-order Butterworth high-pass (corner 0.6x the lowest fundamental) and low-pass (corner 4x the highest fundamental, capped below Nyquist), each blended with the dry signal, giving a flat cut of up to 24 dB at strength 1. Measured through the engine at strength 1, standard tuning at 440 Hz: E2 -0.68 dB, 440 Hz -0.03 dB, E6 -0.10 dB, 3x E6 -1.24 dB, one octave below the low corner -20.5 dB, one octave above the high corner -21.7 dB.
- Not exposed to the user and not applied by the app shell to spectrograms or analysis: the filter only changes what is heard, like mute and solo. No change outside `crates/audio`.

## 2026-11-02 - analysis crate: spectrogram

Workspace (root `Cargo.toml`): added `rayon = "1"` and `image = { version = "0.25", default-features = false, features = ["png"] }`; `[profile.dev.package.fretscribe-analysis] opt-level = 3` so the transform is usable in debug builds. No change in `core` or `audio`.

New public API in `fretscribe-analysis` (all in project time, seconds):
- `compute_spectrogram(&[f32], sample_rate, &SpectrogramParams) -> Spectrogram` and `compute_spectrogram_streaming(Arc<[f32]>, sample_rate, &params) -> SpectrogramStream { info, chunks }` (chunks arrive in time order; apply with `Spectrogram::empty(&info)` + `apply_chunk`; dropping the receiver cancels).
- `Spectrogram { data: Vec<f32> 0..1, n_frames, n_rows, hop_sec, min_midi, max_midi, bins_per_semitone }`, row-major, row 0 = lowest pitch, frame `i` centred at `i * hop_sec`. Row `r` is centred on grid pitch `min_midi + r / bins_per_semitone` relative to A4 = 440 Hz, whatever the project's reference pitch. The grid is never recomputed when the reference changes: `row_to_midi` / `midi_to_row` (methods, they need the grid) take the `PitchRef`.
- `render_rgba(time_range_sec, w, h, midi_range, &PitchRef, &Lut, Levels, gamma)`: average downsampling when zoomed out, bilinear when zoomed in; image row 0 = highest pitch.
- `APP_GAMMA = 0.8` and `APP_WHITE = 0.8` (black 0.0): the application's starting display values; the identity defaults (`Levels::default()`, `GAMMA_DEFAULT = 1.0`) are separate.
- `Spectrogram::suggested_levels()` returns black 0.0 and white at the strongest value of the data, for when a track is first loaded (falls back to `APP_WHITE` for silent or empty data).
- `Levels` (`new`, `clamped`, `to_db`, `MIN_LEVELS_GAP`), `apply_levels`, `apply_gamma`, `clamp_gamma`, `GAMMA_MIN/MAX/DEFAULT`; `analysis::lut::{Lut, discover_luts}`; `AnalysisError`.
- Palettes are PNGs in the repository-root `LUT/` folder (`Classic.png`, `Heat.png`, regenerated by `cargo run -p fretscribe-analysis --example make_luts`). The folder must ship next to the executable; `Lut::builtin()` is only the fallback.

Design notes: half-octave bands, each with its own FFT size and a Hann window of `q / f_lowest_of_band` seconds (clamped), so every row has at least the resolution `q` promises (at most 1.4x less time resolution at the top of a band). Long-window bands are evaluated every `window / 8` frames and interpolated in time. A 300 s file takes about 1.8 s in release on 2 cores.

`cargo test -p fretscribe-analysis --test export_spectrogram_png -- <args>` is a plain `fn main()` (`harness = false`); it also ignores `--nocapture`, `--quiet` and `-q` so `cargo test --workspace -- --nocapture` keeps working.

## 2026-11-03 - analysis crate: waveform peaks

No workspace dependency change (`rayon` and `image` were already in `fretscribe-analysis`). The `[[test]] export_waveform_png` target is declared with `harness = false`.

New public API in `fretscribe-analysis` (module `waveform`, also re-exported at the crate root):
- `WaveformPyramid::build(&[f32], sample_rate)`: min/max/sum-of-squares blocks, base block `BASE_BLOCK = 64` samples, each level merging pairs of the previous one; built in parallel with rayon. Accessors `sample_rate()` and `duration_sec()`.
- `WaveformPyramid::peaks((start_sec, end_sec), pixel_width) -> Vec<(min, max, rms)>`: exactly `pixel_width` columns, from the coarsest level whose blocks fit in a pixel. Zero width gives an empty vector; invalid or empty ranges, and the parts of a range outside the audio, give silent `(0, 0, 0)` columns. The finest resolution is one base block (no raw samples are kept), so zooming in beyond 64 samples per pixel repeats blocks instead of leaving gaps. Non-finite samples count as 0.
- `waveform::render_rgba(&[(min, max, rms)], height, &Lut) -> Vec<u8>` (crate-root alias `render_waveform_rgba`): one column per peak, row 0 on top, background = `lut.colors[0]`, a centre line, the min..max envelope coloured by the column's loudness and a brighter RMS body. Amplitude goes through `apply_gamma` with the module constant `waveform::WAVEFORM_GAMMA` (1.0), which is deliberately not a parameter or setting. The colour level is smoothed along the time axis by a moving average of `waveform::WAVEFORM_COLOR_SMOOTHING_PX` pixels (15; 0 or 1 disables it; window `2 * (n / 2) + 1`), also a hand-tuned constant. Both the envelope and the RMS body also fade vertically, most intense on the centre line and `EDGE_SCALE` (0.4) times that at the tips (private constants `EDGE_SCALE`, `BODY_LIFT`, `ENVELOPE_SHARE` in `waveform.rs`); the body stays brighter than the envelope even at full scale. Only colours are affected, the envelope and body outlines are unchanged, and silent columns are left out of the average so notes do not fade at their edges.
- `Lut::color(t) -> [u8; 4]` (in `lut.rs`): lookup for `t` in 0..=1, clamped, NaN gives the weakest colour. Same index maths as the spectrogram renderer.

Tests and tooling: unit tests in `waveform.rs` and `lut.rs`; `cargo test -p fretscribe-analysis --test export_waveform_png -- --audio <file> --seconds <n> [--start <s>] [--lut Classic|Heat] [--width <px>] [--height <px>] [--out <png>]` writes a PNG (prints the usage and succeeds without arguments); `cargo run --release -p fretscribe-analysis --example waveform_peaks -- <file>` prints peaks.

## 2026-11-03 - ui crate: measure grid (core additions)

Backward compatible; no existing signature or behaviour changed.
- `Propagation { Following (default), Single }` and `set_tempo_scoped` / `set_time_sig_scoped`, both as functions in `measures` and as `Project` methods (`Project::set_tempo_scoped(index, bpm, propagation, policy)`, `Project::set_time_sig_scoped(...)`). `set_tempo` / `set_time_sig` are thin wrappers using `Following`. `Single` edits only `index`, which backs the "apply to following identical measures" checkbox of the grid popup.
- `UndoStack` (`undo` module, also re-exported with `DEFAULT_UNDO_LIMIT = 200`): linear history of `ProjectEdit`. `push`, `undo(&mut Project) -> Option<&ProjectEdit>`, `redo(...)`, `can_undo`, `can_redo`, `undo_label`, `redo_label`, `clear`, `with_limit`. Pushing discards the redo branch and the oldest edits are dropped beyond the limit. It is not generic: every edit already produces a `ProjectEdit` snapshot pair, so `UndoStack<ProjectEdit>` would be the only instantiation.

## 2026-11-05 - ui crate: overview snapping

- Snapping is on by default and Alt disables it while dragging. The seek point (click and drag) and the region edges (create, resize, move) snap to `snap_points` (default: `measure_starts`); the viewport box does not. No points, no snapping. `DragContext::snap` is a `&[f64]`.
- The widget has a magnet button: `OverviewTimeline::snapping(bool)` gives its state and `OverviewEvent::SnapToggled(bool)` reports clicks (the app stores the state). With the button off, Alt enables snapping instead.

## 2026-11-05 - ui crate: view navigation

No change in `core` or other crates. New `fretscribe_ui::nav` (re-exported: `ViewController`, `ViewTuning`, `NavContext`, `NavRegions`):
- `nav::ViewState { scroll_sec, scroll_y, pixels_per_sec }` is the navigation state (`From<core::ViewState>`, `timeline() -> TimelineView`). `scroll_y` is not persisted (core's `ViewState` is unchanged).
- `ViewController`: input moves `target`, `current` chases it; widgets read `current` only. Per frame: `ctl.run(ctx, &NavContext)` (reads wheel and middle-drag, then animates and requests a repaint while running). Also `set_target`, `jump_to` (centres, animated), `update(dt) -> bool`, `offset_y(ppp)` (vertical offset in whole physical pixels, for drawing), and `pitch.current` (shared pitch range, set via `PitchRange` bounds in `new`).
- All feel constants are in `nav/tuning.rs` (`ViewTuning::DEFAULT`); `min_pitch_span_semitones` was added to the list of the prompt. The JSON loading for live tweaking was not done.
- Lane geometry assumption (Prompt 6b was not available): every strip height is a whole multiple of the strip pitch P in physical pixels, a lane is `(k + 1) * P` (`k = 0` is a collapsed spectrogram, no strip), borders are drawn inside the strip. The app passes `content_px` and `strip_pitch_px` in `NavContext`.
- `run` consumes only the wheel events it uses, by clearing egui's pre-mapped scroll deltas; egui's keyboard UI zoom is switched off by the app with `ctx.options_mut(|o| o.zoom_with_keyboard = false)`.
- Zoom rule update: a fitting hovered measure drifts towards the cursor by at most its pixel-width change per notch (centre reaches the cursor once within that distance), then is clamped inside the edge margins, flush against the side it would have overflowed. A measure that no longer fits is flush with the edge nearest the cursor (edge margin applied), the overflow going off the opposite side; with no measure under the cursor the cursor time is the anchor.
- Zoom rule update 2: a hovered measure wider than the view slides towards being flush with the edge nearest the cursor by at most `max_slide_fraction` of the view width per notch (plus the drift the zoom itself causes), so it no longer races across when the cursor crosses the middle; once flush its edge is the anchor. Beyond `cursor_zoom_ratio` usable view widths the measure is ignored and the zoom anchors on the cursor time (both new `ViewTuning` fields).

## 2026-11-06 - analysis crate: beat fit

No change in `core` or other crates, no workspace dependency change (`realfft` and `rayon` were already used). New `fretscribe_analysis::beatfit` (not re-exported at the crate root):
- `onset_envelope(mono, rate) -> OnsetEnvelope { values, hop_sec }` (value `k` is at `k * hop_sec` of the track's audio time; `at(sec)` interpolates), `detect_onsets(&env) -> Vec<f64>` (works on any envelope, so per helper track too).
- `combine_envelopes(&[EnvelopeInput { env, offset_sec, weight }])` into project time (frames before 0 are dropped); `default_weight(TrackRole)` and `envelope_input(&Track, env) -> Option<EnvelopeInput>` (`None` when `use_for_beats` is off; uses `track.offset_sec`). The app decides which tracks, including the source guitar, have `use_for_beats` set.
- `fit_measures(&env, &measures, max_shift_beats) -> Vec<MeasureFit { index, new_start_sec, confidence }>`; `fitted_grid(&measures, &fits) -> Vec<FittedMeasure>` refits every tempo to the fitted spans and feeds `Project::auto_fit` (callers normally pass `KeepAudioPosition`).
- `estimate_tempo(&env, (lo, hi)) -> Vec<(bpm, score)>` (top 3, ranked with a mild log-Gaussian prior around 120 BPM) and `octave_ratio(a, b)` for the double/half-time ambiguity.
- `beatfit::synth` is `#[doc(hidden)]` test material shared with the `beatfit_bench` example.

## 2026-11-06 - analysis crate: track sync

No change in `core` or other crates, no workspace dependency change (`rubato` is not used: `estimate_offset` takes one sample rate for both signals, so the caller resamples). New `fretscribe_analysis::sync` (not re-exported at the crate root); it reuses `beatfit::{onset_envelope, estimate_tempo, detect_onsets, octave_ratio, HOP}` for tempo, drift and grid-hint onsets and has its own 3-band envelope for the correlation.
- `assume_synced(ref_sec, other_sec, tolerance_ms)` (`DEFAULT_TOLERANCE_MS = 20`).
- `estimate_offset(reference, other, rate, SyncOptions { max_lag_sec (default 15), grid_hint: Option<&[Measure]> }) -> Vec<SyncCandidate { offset_sec, confidence, note }>`: at most 3, best first; empty for too short or silent input. `project_time = audio_time + offset_sec`. Notes: "ambiguous by one beat (about 0.52 s)" (confidence of the best is scaled by `1 - lower/higher`, unless `grid_hint` decides) and a drift note with the tempo ratio.
- `snap_offsets(other_onsets, targets, current_offset_sec, max_dist_sec)`: offsets within `max_dist_sec` of the current one, nearest first.
- Cost: the signal-level refinement is one FFT correlation over the full signals (about 300 MB and a few seconds for two 3-minute tracks at 44.1 kHz).
- `cargo run --release -p fretscribe-analysis --example sync_offset -- ref.wav other.wav [--max-lag 15]`.

## 2026-11-06 - analysis crate: reference pitch estimation

No change in `core` or other crates, no workspace dependency change. New `fretscribe_analysis::pitchref` (not re-exported at the crate root); conversions go through `core::cents_off` and `core::ref_from_cents`.
- `estimate_reference(mono, rate, region: Option<(f64, f64)>, PitchRefOptions { max_analysis_sec: 90, min_hz: 80, max_hz: 2000 }) -> PitchRefEstimate { cents, a4_hz, confidence, histogram }`. It only estimates: the app decides whether to propose `a4_hz`. `cents` is the offset from the 440 Hz grid within -50..=+50 (positive: sharp); a recording a whole semitone off, or played with a capo, reads about 0 (that is the tuning and capo, not the reference).
- `histogram`: 100 circular 1-cent bins over -50..+50 (bin `i` centred on `i - 49.5`), smoothed, normalised to a sum of 1 (all zero when nothing was found).
- Without `region`, a file longer than `max_analysis_sec` is analysed on its most tonal stretch of that length. An invalid region (non-finite) is ignored.
- Confidence is robust to mild broadband noise (the long window gives the tonal peaks about 40 dB of gain); it drops with heavy noise, vibrato or bends, an out-of-tune guitar, noise and drums.
- `cargo run --release -p fretscribe-analysis --example pitchref -- file.wav [--start s --end s]`.

## 2026-11-07 - ui crate: spectrogram view

- `fretscribe-ui` now depends on `fretscribe-analysis` (`SpectroStyle` uses `Levels`/`Lut`; the widget calls `Spectrogram::render_rgba`) and on `fretscribe-notation`. This departs from the earlier "ui does not depend on analysis" comment in `overview/waveform.rs`; the waveform strip still takes its peaks from a closure.
- `fretscribe-notation` was empty: added the provisional `tab::{lowest_fret_candidate, candidate_label}` ("E 10" format, string 0 = lowest, capo respected) for the hover readout. Replace with the real tab inference when it lands, keeping the signatures.
- New `fretscribe_ui::spectro`: `SpectrogramView` (builder, `show(ui, &TimelineView, &PitchView, &mut SpectroCache) -> SpectroOutput`), `SpectroStyle`, `SpectroEvent::{AddNoteRequested, SeekRequested}`, `GhostNote`, `HitZone`, `hit_test`, `SpectroGeometry`, `WaveformView`, `show_time_ruler`. `view::PitchView { min_midi, max_midi }` (`From<nav::PitchRange>`).
- Notes come as `&[Note]` plus the core `NoteTimes` (project seconds). `link_to` goes from the start note to the next one and the connector kind is the start note's `technique` (Slide, Bend; anything else is a plain line).
- The texture cache key includes window, pixel size, pitch range, A4, levels, gamma, LUT name, spectrogram data pointer and a caller `revision` (bump it when the data changes in place, e.g. streamed chunks).
- Pitch guides are one band per semitone (alternating +5% white; the band under the cursor is brighter), no crosshair.
- `PITCH_RULER_WIDTH` (left strip with the pitch labels) is exposed as `SpectroOutput::pitch_ruler` for the nav drag; clicks there add no note.

## 2026-11-07 - ui crate: track lanes

No change in `core`. New `fretscribe_ui::tracks` (`TrackLanes`, `TrackLane`, `TrackHeader`, `TrackLanesState`, `LaneOptions`, `LaneSource`, `TrackEvent`) and `view::LANE_HEADER_WIDTH` (240 pt).
- Geometry in whole physical pixels: `WAVE_H_PX = 66`, `BORDER_PX = 1`, `STRIP_PITCH_PX = 67` (pass it as `NavContext::strip_pitch_px`), `SPECTRO_ROWS = 5`, resizable 1..=16 rows by dragging the bottom edge of a spectrogram. Collapsing steps through whole rows, so every strip top stays a multiple of the pitch; the "+" row is one pitch tall. `TrackLanesState::strips` gives the strips and `content_px` for the navigation before `show`; (`NavRegions` no longer takes the source strip).
- Timeline x = 0 is the right edge of the header column: the ruler and the grid must start `LANE_HEADER_WIDTH` to the right of the lane area's left edge. `TimelineView` stays in project time; only a lane's spectrogram and waveform are shifted (`SpectrogramView::audio_offset`, `WaveformView::audio_offset`). Notes are never shifted.
- Audio comes through the `LaneSource` trait (spectrogram, peaks, duration, onsets, `snap_candidates` backed by `analysis::sync::snap_offsets`), all in the track's own audio time. The reference track and `ghost` are passed by the app; `LaneOptions` (notes on all lanes, onsets, unlinked pitch, "move notes") are display preferences owned by the app, not project data.
- Added to the listed events: `OffsetDragBegan(id)` / `OffsetDragEnded(id)` bracket the live `OffsetChanged` events of one drag so the app can make it a single undo step. Typed and nudged values send one `OffsetChanged` each; the reset button sends offset 0. `OffsetChanged::move_notes` is true only for the source track while the menu option is on; the app applies it with `core::shift_track`.
- Snapping is within 8 px (`SNAP_TOLERANCE_PX`) of the raw dragged offset; Alt disables it, Shift drags 10x slower. Clicks on a helper lane's spectrogram come back as `SpectroEvent::SeekRequested`, never `AddNoteRequested`.
- The offset field commits on Enter; Esc or clicking away reverts. Offsets are limited to +-60 s.

## 2026-11-07 - ui crate: Alt + wheel resizes the lane

No change in `core`. Alt + wheel over a lane changes its configured spectrogram rows (wheel up = taller, 1..=16), replacing the Alt + wheel pitch zoom; the pitch range is always shown in full.
- Removed: `NavRegions::source_spec` and `TrackLanes::source_spec`, `ViewController::zoom_pitch`, `ViewTuning::{pitch_zoom_per_notch, min_pitch_span_semitones}`, `WheelActions::pitch`, `LaneOptions::unlink_pitch` and `LaneState::pitch`. `ViewController::pitch` stays (always the full range) and `PitchView` keeps feeding the spectrograms.
- The nav no longer claims Alt + wheel; the lane reads it.


## 2026-11-07 - ui crate: animated lane resize

No change in `core`. Alt + wheel notches are read from the wheel events with the nav's notch conversion (one notch = one row). Wheel resizes animate (`RESIZE_ANIM_SEC`, whole rows, like collapsing); dragging the bottom edge still follows at once. `geometry::rows_shown` now takes `rows: f32` (the animated value); `LaneOutput::resized_rows` is `Option<(rows, animate)>`.

## 2026-11-07 - ui crate: smoother lane resize, animated reorder, bottom handle

No change in `core`. Wheel resizes now glide exponentially (`RESIZE_TAU_MS`, retargets smoothly on rapid notches) instead of restarting a fixed-time animation; `RESIZE_ANIM_SEC` is gone. When the track order changes (reorder, add, remove) the lanes slide from their old position (`REORDER_TAU_MS`). The resize handle is the bottom edge of the whole lane (below the waveform), timeline part only.
