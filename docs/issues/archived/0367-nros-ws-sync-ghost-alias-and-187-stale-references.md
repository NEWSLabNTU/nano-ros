---
id: 367
title: "`nros ws sync` is a ghost: the phase-265 rename to `nros sync` left a hidden live alias, a misnamed shell helper, and ~187 stale references"
status: resolved
type: tech-debt
area: build, cli
related: [phase-265, issue-0336, issue-0363]
resolved_in: "cmd/ws.rs Sub::Sync deleted + sweep + check-retired-submodule-refs gate"
---

# `nros ws sync` is a ghost: the phase-265 rename left a live alias and ~187 stale references

## Problem

Phase-265 W5 renamed `nros ws sync` to `nros sync`, but only the happy path
moved. The old spelling survives in three forms:

1. **A live hidden alias.** `cmd/ws.rs` still carries
   `Sub::Sync(SyncArgs)` (`#[command(hide = true)]`), dispatched at
   `ws.rs:174-177` with a one-line deprecation note. The comment says "kept
   for one release cycle" — that was phase-265; the cycle is over.
2. **A misnamed shell helper.** `nros_require_ws_sync` (called from
   `just/native.just`, `just/zephyr-ci.just`, `just/zephyr-dev.just`,
   `just/qemu-baremetal.just`, `scripts/build/workspace-fixtures-build.sh`, …)
   probes for the CAPABILITY and the recipes then correctly run
   `"$nros_cli" sync` — the helper's name is the only thing still saying
   `ws sync`, and it teaches the dead verb to every reader.
3. **~187 textual references** (`git grep -c 'nros ws sync'`, excluding
   vendored trees and archived issues): justfile/`just/*.just` comments and
   `echo` progress lines, `book/src/reference/cli.md` (documents the alias),
   `ci/docker/zephyr-ros/Dockerfile`, the `pr-checks.yml` job name
   `nros new -> ws sync -> resolve`, `scripts/build/fixture-inventory.py`
   notes, active RFCs (0032/0040/0042/0061), and `docs/development/`.

Nobody executes the old verb anymore (every recipe already calls
`nros sync`), but the ghost keeps re-entering new prose because greppable
precedent is how text gets written in this repo — the same drift class as
issue 0336's retired-submodule references, which got a grep gate for exactly
this reason.

## Fix (sweep, not an edit)

- Delete `Sub::Sync` from `cmd/ws.rs` (keep `env`/`list`/`status`/`clean`/
  `doctor`); `nros ws sync` then fails with clap's unknown-subcommand error.
  `run_sync` stays — it is `nros sync`'s implementation.
- Rename `nros_require_ws_sync` → `nros_require_sync` at its definition and
  every call site.
- Sweep every `nros ws sync` / "`ws sync`" reference to `nros sync`
  (comments, echoes, book, Dockerfile, workflow job name, RFC prose,
  `fixture-inventory.py` notes). Internal `ws sync:` log prefixes in
  `cmd/ws.rs` error strings should become `sync:` in the same pass.
- Add `nros ws sync` to a `check-fast` grep gate (extend
  `scripts/check-retired-submodule-refs.sh`'s pattern list or a sibling
  script) so the spelling cannot creep back — per the issue-0196 rule, make
  the gate cover the whole class (`\bws sync\b`), not just the exact string
  swept today.
- Put the sweep command in the commit message
  (`git grep -n 'ws sync' -- . ':!*third-party*'`).

## Non-goals

`nros ws` itself stays — `env`/`list`/`status`/`clean`/`doctor` are live
workspace utilities. Only the `sync` alias inside it is the ghost.

## RESOLVED (2026-08-01)

- Deleted `Sub::Sync` from `cmd/ws.rs` (+ its dispatch arm); `nros ws sync` now
  fails `error: unrecognized subcommand 'sync'`. `run_sync`/`SyncArgs` stay — they
  are `nros sync`'s implementation (`lib.rs` `Cmd::Sync → ws::run_sync`).
- Renamed the shell helper `nros_require_ws_sync` → `nros_require_sync` at its
  definition (`scripts/build/cargo.sh`) and all call sites.
- Swept every `ws sync` in active code + user-facing docs → `nros sync`:
  `ws.rs` log/error prefixes (`ws sync:` → `sync:`), `just/*.just` comments/echoes,
  the book (removed the dead-alias paragraph), `pr-checks.yml` job name, the
  facade/metadata-refresh error strings, `scripts/build/*.sh`, examples. Historical
  records (issue docs, archived roadmap, superpowers specs, audit findings) left as
  the record of the drift.
- Added a **class gate**: `RETIRED_SPELLINGS=("\bws sync\b" …)` in
  `scripts/check-retired-submodule-refs.sh` (already in `check-fast`), so the
  spelling cannot creep back — issue-0196 rule (gate the class, not the string).

Verified: `nros ws sync` errors; `nros ws --help` shows only env/list/status/clean/doctor;
`nros sync --help` works; the gate passes with 0 live references.

Sweep command: `git grep -n 'ws sync' -- . ':!*third-party*' ':!*/archived/*'`
