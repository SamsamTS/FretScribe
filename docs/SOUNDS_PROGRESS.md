# Prompt 17 progress (branch `playback-sounds`; main is untouched)

Resume: read this file and `git log --oneline`, continue at the first unchecked part.

## Parts
- [x] A `core::perform`
- [x] B synth: curves, kinds, presets, demo, tests
- [x] C app wiring, Sound dropdown, docs

## Decisions
- A: an on-the-beat grace note starts with its main note, so "stored times" and "ends before the main note" conflict: it is moved to end at the main start. A grace note slide/bend-linked to its main note is one chain (grace velocity, no re-attack). A chain's kind comes from its first note; palm mute on the last one shortens the end. `perform::value_at` is public (the synth reuses it).

- B: cubic Lagrange fractional delay (allpass cannot move click-free). Added `NoteSynth::reset` (hard silence once faded out) beside `release_all`. Curve pitches below 20 Hz are clamped (fixed delay buffer).
- C: performance cached in `Session`, rebuilt 120 ms after the last change and flushed on play.

## Next step
Done. Awaiting review; preset values still to be tuned by ear.
