# Prompt 17 progress (branch `playback-sounds`; main is untouched)

Resume: read this file and `git log --oneline`, continue at the first unchecked part.

## Parts
- [x] A `core::perform`
- [ ] B synth: curves, kinds, presets, demo, tests
- [ ] C app wiring, Sound dropdown, docs

## Decisions
- A: an on-the-beat grace note starts with its main note, so "stored times" and "ends before the main note" conflict: it is moved to end at the main start. A grace note slide/bend-linked to its main note is one chain (grace velocity, no re-attack). A chain's kind comes from its first note; palm mute on the last one shortens the end. `perform::value_at` is public (the synth reuses it).

## Next step
Part B.
