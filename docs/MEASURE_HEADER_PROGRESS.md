# Prompt 18: measure header (branch `measure-header`)

- [x] A: core
- [ ] B: header widget (`ui`)
- [ ] C: app wiring
- [ ] D: `anchor` icon
- [ ] E: acceptance, contract entry

## Decisions (where the spec and the code disagreed)
- `set_tempo`/`set_time_sig` free functions still change all following measures (Auto tempo uses it; locked ones are now skipped). The header uses the new `Propagation::Identical` (`set_*_with`).
- Core `MeasureFit { end_sec: f64, confidence }`, not `end: TimePos`. `FittedMeasure` is gone.
- Auto-fit never changes `tempo_bpm`; it moves barlines. A barline touching a locked measure and the grid end stay (flag only, no error). A follower no longer matching its predecessor's tempo is anchored so later barlines stay put.
- `set_auto_fit` takes a `NotePolicy`. Manual move clears `auto_fit` on the measure whose start moved (spec DRAFT).
- `cover_audio` keeps trailing locked measures.

## Next step
B: `crates/ui/src/grid`.
