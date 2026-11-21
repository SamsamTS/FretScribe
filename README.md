# FretScribe

Guitar transcription from audio: spectrogram, note editing, staff and tab, multi-track sync.

## Build and run

```
cargo run --release -p fretscribe
```

On Linux the audio backend needs the ALSA development files (`apt-get install libasound2-dev pkg-config`).
`cargo run -p fretscribe-demos --example demo_project -- <dir>` writes a small synthetic project to try the editor with.

## Tests and demos

Everything for development lives in `dev/` (see `docs/CONVENTIONS.md`): `cargo test --release --workspace`, `cargo run --release -p fretscribe-demos --example <name>` (`beatfit_bench`, `make_luts`, `pitchref`, `sync_offset`, `waveform_peaks`, `demo_project`, `ui_preview`, `play`) and `dev/check_without_dev.sh`.

## The `LUT/` folder

Spectrogram palettes are PNG images in the `LUT/` folder, named after the file stem. **The `LUT/` folder must be
copied next to the executable** when packaging (`<exe dir>/LUT`); when running through `cargo run` the one in the
workspace is used. Without it the built-in palette is used and a notice is shown.

## Settings and recovery

Settings and recovery copies live in `%APPDATA%\fretscribe` (`$XDG_CONFIG_HOME/fretscribe` on Linux). The editor
writes a recovery copy of unsaved work every 30 seconds and every 20 edits (configurable in the settings file); after
an unclean exit it offers to restore it.
