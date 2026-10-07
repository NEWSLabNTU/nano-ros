---
id: 1748
title: "A new or edited UNTRACKED CLI source stales the CLI, and `just setup-cli` rebuilds without re-baking the stamp"
status: resolved
type: bug
area: [cli, build, tooling]
severity: low
found: 2026-10-07
related: [1336, 1018, 0627, 0604]
resolved_in: "branch fix/cli-stamp-untracked-watch"
---

## Symptom

Reported by the phase-481 W1 work, then reproduced on `main`:

1. `just setup-cli`; `just check cli-fresh` -> fresh.
2. Create `packages/cli/nros-cli-core/src/zz_stamp_probe.rs` (untracked, not
   compiled) -> `check cli-fresh` reports STALE, correctly.
3. `just setup-cli` -> prints `built:`, but `check cli-fresh` stays
   `STALE — built from <a>, sources are now <b>`.
4. Editing the untracked file and re-running `setup-cli`: still STALE.
   Only `touch packages/cli/nros-cli-core/build.rs` clears it.

## Cause

`source_stamp` folds untracked CLI sources into the stamp
(`untracked_cli_files`, so a new file stales the CLI before it is added — the
intended rule, issue 1018's "tracked and untracked are ONE input"). But
`nros-cli-core/build.rs` emitted `rerun-if-changed` for the TRACKED inputs
(`cli_input_files`, `git ls-files`) and the index only. An explicit list
replaces cargo's default watch, so no edge saw an untracked file — neither an
edit to one, nor a new one appearing. Cargo rebuilt nothing, `setup-cli`
reported success, and the old stamp stayed baked in. Issue 1336's shape (an
input the stamp reads, with no watch), one input over.

## Fix

`build.rs` also watches:

- every existing untracked input, by name;
- each crate's top-level input directory under `packages/cli`
  (`<crate>/src`, `tests`, `packs`, `templates` — derived from where the
  inputs are, never `target/`). Cargo re-runs a build script when anything
  under a watched directory is newer than the last run, which is what a NEW
  file is; a file that does not exist yet cannot be named.

Measured after the fix, same steps: add -> `setup-cli` -> fresh; edit ->
`setup-cli` -> fresh; remove -> `setup-cli` -> fresh; an unchanged
`setup-cli` rebuilds nothing.
