# WIP (delete before final commit) - links/grace rework on prompt-7b-legato-grace
Design: A.end == B.start for every link (core `write_link` ties it, `resolve_overlaps` re-ties partners of placed notes).
Exception: on-the-beat grace notes (Appoggiatura) keep their own length. `LinkType::Dashed` = grace only, implicit (no data).
Slide/bend/dashed source = band (<= h+pad, <= half the way to B.start), right edge not resizable.
Steps: [ ] core links/grace/overlap  [ ] glyph layout  [ ] editor (draw/resize/move/grace move/cycle/H toggle/Grace button)  [ ] tests  [ ] CONTRACT_CHANGES  [ ] rm WIP.md
