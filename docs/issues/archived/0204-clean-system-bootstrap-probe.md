---
id: 204
title: "No automated clean-system bootstrap verification — book setup steps are never executed on a pristine host"
status: resolved
type: task
area: ci
related: [rfc-0014, issue-0200]
---

## Problem

The book's bootstrap + per-platform setup instructions (clone → `direnv
allow`/`source ./activate.sh` → `just setup` → platform SDK provisioning per
RFC-0014) are only ever exercised on already-provisioned developer hosts.
Nothing runs them on a clean system, so missing-prereq regressions (a new
implicit host dependency, a setup recipe that assumes prior state, a doc step
that rotted) surface only when a new user hits them.

The `/audit` skill's F3 check (added 2026-07-16) statically cross-reads the
book against `activate.sh` / `justfile` / `nros-sdk-index.toml` — that
catches doc drift but NOT real-world breakage. This issue is the dynamic
half.

## Wanted

A containerized probe (fresh Ubuntu LTS image, no toolchains) that executes
the book's documented setup steps verbatim and asserts the documented
outcome:

1. Bootstrap: clone, activate, `just doctor` green, `just setup-cli`.
2. At least one cheap platform lane end-to-end: e.g. native fixture build +
   one runtime test (`just native build-fixtures` is disk-heavy — pick the
   smallest honest slice, or gate the fat lanes to the #200-class big runner).
3. Fail loud on any undocumented prerequisite (the probe image gets NOTHING
   the book doesn't install).

## Constraints

- Runner-class work: disk/network budget overlaps issue #200's CI-runner
  campaign — likely the same infrastructure.
- Steps must be extracted FROM the book (or the book generated from the
  probe script) so the probe can't drift from what users actually read —
  a probe with its own hand-rolled steps re-creates the F3 drift problem
  one level down.

## Resolution (2026-07-16)

Landed as designed (book = SSoT via extraction, option A):

- **Tagged book blocks**: fenced blocks carrying a `probe=NN` info-string
  token (` ```sh probe=10 `) in `installation.md` (new host-prereqs apt
  block + the whole-flow block) and `first-node-rust.md` (Build block, which
  gained the previously missing `nros sync` step). mdBook renders them
  unchanged.
- **`scripts/probe/extract-book-steps.py`**: extracts tagged blocks in NN
  order into one bash script; `--subst` (exactly-once literal) rewrites the
  pinned-tag clone line + the `<board>` placeholder, failing loudly on book
  drift.
- **`scripts/probe/run-bootstrap-probe.sh`**: runs the script in a pristine
  `ubuntu:24.04` container (clone from the RO-mounted checkout by default;
  `PROBE_CLONE_URL`/`PROBE_BRANCH` for remote). Only host shims: `sudo` +
  a `safe.directory` gitconfig. `scripts/probe/verify-first-node.sh`
  (probe-owned; the book's Run section is interactive) re-sources
  `activate.sh` ("open terminal 1") and asserts the chapter's documented
  readiness signal.
- **Wiring**: `just probe bootstrap` / `just probe extract`; nightly.yml
  `bootstrap-probe` job (07:00 cron + dispatch, not per-PR).
- Deviation from the original sketch: the probe asserts the BOOK's front
  door (`bootstrap.sh`, no `just`) rather than `just doctor`/`setup-cli`,
  per this issue's own steps-from-the-book constraint; the cheap lane is
  the first-node talker (fixture builds are #200-class).

First run caught four real regressions, all fixed:
1. `activate.sh`/`.fish` never wired `~/.cargo/bin` — the bootstrap shell
   lost cargo and `nros setup`'s zenohd source build failed (e4103a979).
2. Bundled `std_msgs`/`builtin_interfaces` were silently LOST with the
   retired `packages/codegen` submodule; `nros sync` never consulted the
   bundled fallback at all → no-ROS hosts hit crates.io's yanked ROS
   crates. Re-vendored at `packages/cli/interfaces/` + `sync` now uses
   `load_index_with_fallback` (a5a5947ba).
3. SDK-store `zenohd` was never on PATH despite the book's `zenohd`
   instruction — whitelisted in the activate files (7f681a1c7).
4. The book omitted `nros sync` before the first `cargo build` (book fix in
   the probe-tagging commit).

PASS: talker prints `Publishing: 'Hello World: 1'` via zenohd on a pristine
container, end-to-end from the book text alone.
