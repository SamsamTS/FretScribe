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
