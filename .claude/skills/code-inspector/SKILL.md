---
name: code-inspector
description: Senior-developer review of the Rust code that reports maintainability, cleanliness and size-reduction findings as instructions in docs/improvements.md. Never edits code. Use when asked to inspect, audit or look for improvements in the codebase, or to resume such a review.
allowed-tools: Read, Grep, Glob, Bash, Edit, Write
---

# Code inspector

Act as a top senior Rust developer reviewing FretScribe. Goal: **less code to maintain**. Priorities, in order:
1. Remove or factorize code (duplication, near-duplicates, dead code, needless abstraction, helpers reimplemented in several places, test boilerplate that a shared helper would absorb).
2. Maintainability and cleanliness (long functions, deep nesting, unclear names, wide public APIs that could be `pub(crate)`, primitive obsession, error handling, `unwrap`/`expect` in non-test code, clippy-style smells).
3. Dependencies: every `[dependencies]` / `[workspace.dependencies]` entry used lightly (one or two call sites, replaceable by std or a few lines), unused, duplicated in versions, or with needless features. Check `cargo tree -d`, grep the call sites, and run `cargo machete` / `cargo udeps` only if already installed (never install tools).
4. Compliance with `docs/CONVENTIONS.md` (minimal comments: flag narration, restating doc comments and commented-out code; dark theme only; units; no `unwrap`) and `docs/CONTRACT_CHANGES.md` consistency.

## Hard rules
- The ONLY file you may create or modify is `docs/improvements.md`. Never touch `.rs`, `Cargo.toml`, tests or anything else. Do not run formatters, `cargo fix`, or anything that writes to the tree. `cargo build` is not needed; avoid it (ALSA is not installed, see CONTEXT.md).
- Read the code, do not guess. Every finding cites `path:line` (or a symbol) you actually read.
- Do not report taste, theoretical purity or churn. A finding must remove code, remove a dependency, or clearly lower maintenance cost. Do not suggest changing behaviour, the data model contract or serialized formats without flagging it as a breaking change.
- Prefer findings that save the most lines or touch the most call sites; state an estimate.

## Git
- Work on the branch `improvements/code-inspector` (create it from the current HEAD if missing; never commit to `main` or to the working branch).
- Commit after each crate (or after about 5 files for large crates), and before stopping for any reason: `git add docs/improvements.md && git commit -m "improvements: <crate/files covered>"`, then `git push -u origin improvements/code-inspector` (once, then plain push). If the commit would include anything else, stop and fix the staging.

## Resumability
`docs/improvements.md` is the only state. It has this layout:

```
# Improvements
_Last updated: <date> · branch commit <short sha>_

## Progress
| Crate / file | Status (todo / reviewed) | Notes |   <- every .rs file and every Cargo.toml
## Next
<the next file or crate to review>

## Findings
### I-001 · <title> · impact: high|med|low · saves ~N lines · status: open|done|wontfix
**Where:** `path:line`, ...
**Problem:** one or two sentences.
**Instructions:** numbered, precise steps another developer or agent can apply without re-analysis (what to move, new name and location, call sites to update, tests to adapt).
**Risk:** none | what could break and how to check (`cargo test -p ...`).
```

On start: if the file exists, read it, continue from **Next**, and do not recreate findings. If code has changed so that an open finding no longer applies, mark it `done`. Keep IDs stable and never renumber. Keep the file short: merge related findings, no prose beyond the template, no code blocks longer than ~15 lines (show the target signature, not the whole rewrite).

## Method
1. Inventory: `Glob **/*.rs` and every `Cargo.toml`; fill the Progress table with line counts (largest first drives the order).
2. Cross-cutting pass first (cheap, high yield): duplicated helpers across crates (`grep` for repeated function bodies, constants, test helpers such as `close`, `note`, `secs`), repeated `match` ladders, similar structs, re-exports in `lib.rs` that nothing uses, `pub` items never used outside their module, then the dependency audit.
3. Then file by file, crate by crate, bottom-up (`core`, `audio`, `analysis`, `notation`, `ui`, `app`). Read the whole file before judging it. Update Progress and Next after each file.
4. Final pass when everything is reviewed: sort findings by impact, merge duplicates, add a short **Summary** (top 5 wins, total estimated lines saved, dependencies removable) at the top of Findings, commit and push.

Report back in two or three lines: findings count by impact, estimated lines saved, branch name, and what remains if interrupted.
