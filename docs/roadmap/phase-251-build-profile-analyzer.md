# Phase 251 — passive build-profile analyzer (`nros-build-profile`)

Status: **Implemented (2026-06-16)** — P1–P4 landed; crate at
`packages/testing/nros-build-profile`, driven by `just profile`. · Design:
`docs/superpowers/specs/2026-06-16-build-profiling-design.md` · Audit:
`docs/development/build-ux-audit.md`

## Why

A user's nano-ros build runs on a framework toolchain (`west`, `cmake`, `idf.py`,
`cargo`) and is a black box for time attribution — no stage breakdown, no per-unit
detail, no redundant-rebuild signal. `nros` is deliberately setup+codegen only and does
not own the build, so the profiler must be a **passive, read-only** tool that parses the
timing artifacts the native build already emits.

The audit established the enabling fact: across the whole platform matrix, deep timing
data collapses to **two artifact formats** — ninja's `.ninja_log` (west/cmake/idf, no
opt-in) and cargo's `--timings` HTML (`UNIT_DATA` JSON). Two parsers cover everything.

## Direction (user, 2026-06-15)

- Keep `nros` scoped to setup + codegen. **No `nros build` / `nros test` verb.** nano-ros
  stays an external dep built by the RTOS frameworks' own toolchains.
- Profiler is a separate parser lib + bin, driven primarily by a `just profile <dir>`
  recipe; the bin stays runnable standalone for external copy-out projects.
- Coarse stage timing for every backend; deep per-unit drill-down where the backend emits
  it. Light, actionable diagnostics on top of the numbers.

## Scope

New host crate `packages/testing/nros-build-profile/` (main workspace, lib + thin bin):

- **Collectors** — `ninja_log`, `cargo_timings` (see design §Components).
- **Normalizer** — merge to one `BuildProfile { backend, total_s, stages, units,
  captured_deep }`.
- **Diagnostics** — 4 data-driven rules: cold-C-build, shared-crate-recompiled-N×,
  isolated-`target/`, job-count-vs-RAM (issue #57 budget).
- **Reporter** — stage table (always), `--deep` drill-down, hints (default-on,
  `--no-hints`), `--json` → `nros-build-profile.json`.
- **`just profile <dir>` recipe** — analysis-only (does not build); deep cargo data via
  `NROS_PROFILE=1` injecting `cargo build --timings` in the in-repo build recipes.

## Work items

### P1 — crate skeleton + collectors
- W1.1 Scaffold `packages/testing/nros-build-profile/` (lib + bin), wire into the main
  workspace `Cargo.toml` members.
- W1.2 `ninja_log` collector: parse `.ninja_log` v5/v6, per-output durations, ext-based
  kind classification. Discovery across `build*/` dirs.
- W1.3 `cargo_timings` collector: locate newest `cargo-timing-*.html`, scrape `UNIT_DATA`
  JSON, map build-script → codegen.
- W1.4 Fixture artifacts checked in (`tests/fixtures/sample.ninja_log`,
  `cargo-timing.html`) + per-collector unit tests.

### P2 — normalizer + diagnostics
- W2.1 `BuildProfile` type + normalizer (stage sums, total span, `captured_deep`).
- W2.2 Backend detection (Ninja{West|Cmake|Idf} | Cargo | Mixed) from artifact provenance.
- W2.3 Diagnostics rule set (4 rules), each independent + suppressible. Unit tests on
  hand-built `BuildProfile` values.

### P3 — reporter + CLI
- W3.1 Stage table renderer; `--deep` top-N unit drill-down with bars + missing-deep note.
- W3.2 Hints rendering (`--no-hints`); `--json` writer.
- W3.3 CLI arg parsing (`<dir> [--deep] [--json] [--no-hints]`); actionable
  no-artifacts/partial/malformed error paths.
- W3.4 Reporter golden test.

### P4 — just integration + docs
- W4.1 `just profile <dir> [flags]` recipe (analysis-only; builds the analyzer bin,
  runs it against an already-built dir). **Done.** The `NROS_PROFILE=1` `--timings`
  injection into platform build recipes was **dropped** as scope: it would touch many
  recipes and pushes build orchestration into `just`, against the lean external-dep
  stance. Instead the cargo `--timings` opt-in is **documented** (one flag the user
  adds to their normal `cargo build`), and the analyzer degrades to a coarse table +
  a one-line hint when it is absent.
- W4.2 Integration test (`tests/integration.rs`): `analyze()` against staged
  prebuilt-artifact dirs — exercises real `.ninja_log` / cargo-timings discovery. **Done.**
- W4.3 Book page `book/src/user-guide/build-profiling.md` (+ SUMMARY entry) — the three
  usage flows. **Done.**
- W4.4 CLAUDE.md "Where things live" pointer row. **Done.**

## Acceptance

- `just profile examples/zephyr/rust/talker` (after a `west build`) prints a stage table
  with codegen/compile/link percentages and at least one hint, sourced from `.ninja_log`,
  with **no rebuild**.
- `cd examples/native/rust/talker && cargo build --timings` then `just profile … --deep`
  shows a per-crate cargo-timings drill-down.
- `--json` emits a `nros-build-profile.json` a CI step can diff across commits.
- A cargo build **without** `--timings` still produces a coarse table and a one-line hint
  to enable deep data (no failure).
- All tests pass with **no compilation at test time** (fixture artifacts only; the one
  integration consumes a prebuilt example).
- `nros` gains **no** build/test verb; the crate is independent of the `nros` CLI surface.

## Real-data validation (2026-06-15)

Ran the analyzer against live build artifacts (not just fixtures):

- **Real Zephyr `west` build** (`build/phase212-mf3-zephyr-rust/.ninja_log`): driver
  detection correct (`ninja (west)`). **Found + fixed a correctness bug:** a ninja edge
  with multiple outputs writes one `.ninja_log` line per output, all sharing
  `(start, end, cmdhash)`; the first cut counted each line → a corrosion cargo edge
  (`.a` + stamp + generated `.h`) was counted 3–4×, inflating totals (stage sums ≈ 210 s
  vs a 35 s build). Fixed by keying units on the **edge** `(start, end, cmdhash)` and
  classifying from the union of its outputs (a `*_cargo_build`/`.rlib` edge → compile,
  not link; generated headers → codegen). Post-fix stage sums (~40 s) track the wall
  span (~35 s). Locked with the `multi_output_edge.ninja_log` fixture + test.
- **Real cargo `--timings`** (`examples/native/rust/talker`): total 32.8 s matched
  cargo's own "Finished in 32.81 s"; the `zpico-sys` build script (zenoh-pico C compile,
  16.8 s) correctly attributed to codegen and flagged as the dominant unit; `thiserror
  compiled 6×` surfaced the isolated-`target/` rebuild. No fixes needed.

Both backends now verified on live artifacts. The validation gap is closed.

## Out of scope (deferred)

ETA/prediction, historical trend DB, web UI, sccache-stats integration (hint only),
flash-internal timing, profiling the repo's own fixture matrix.
