# Build Profiling Analyzer (`nros-build-profile`) — Design

- **Date:** 2026-06-16
- **Status:** Draft (design approved in brainstorming; awaiting spec review)
- **Area:** build tooling (`packages/testing/nros-build-profile`)
- **Audit:** `docs/development/build-ux-audit.md`
- **Roadmap:** `docs/roadmap/phase-251-build-profile-analyzer.md`

## Problem

A user building their own nano-ros node project gets no time attribution from the
build. The build runs on a framework toolchain (`west`, `cmake`, `idf.py`, `cargo`),
and `nros` covers only setup + codegen — so there is no place that reports where the
40 s went (codegen vs compile vs link vs flash), which unit dominated, or whether a
shared crate was rebuilt redundantly. See the audit for the full inventory.

## Goals

- A **passive** profiler: the build runs unchanged on its native toolchain; the tool
  only **reads** the artifacts the build already emits. It never compiles or flashes.
- **Cross-backend** from day one: native + cross cargo, Zephyr `west`, cmake C/C++,
  esp32 `idf.py`. Coverage rides on the two artifact formats the audit identified.
- **Coarse always, deep when available**: a stage table for every build; per-unit
  drill-down where the backend emits it.
- **No new `nros` build/test verb.** nano-ros stays an external dep; `nros` stays
  setup+codegen.

## Non-goals (v1)

ETA/prediction; historical trend DB; web UI; sccache-stats integration (hint only);
flash-internal timing (coarse wall-clock only); profiling the repo's own fixture matrix
(that already has the `build-test-fixtures` joblog).

## Architecture

A host crate `packages/testing/nros-build-profile/` in the **main** workspace (plain
`cargo`, sibling to `nros-tests`/`nros-bench`/`nros-smoke`). Lib + thin bin. Primary
front-door is a `just profile <dir>` recipe; the bin is also runnable standalone
(`nros-build-profile <dir> [--deep] [--json]`) for external copy-out projects with no
justfile.

Pipeline:

```
normal build (west / cargo / cmake / idf.py)
   └─ emits native artifacts  (.ninja_log [free] | cargo-timings [--timings])
just profile <dir>
   └─ Collectors → Normalizer → Diagnostics → Reporter → stdout (+ optional JSON)
```

### Components (each isolated, independently testable)

**1. Collectors** — one per artifact format; each discovers files under a project dir
and parses them into `Vec<RawUnit>` plus a backend tag. The pair covers the matrix:

- `ninja_log` — parses `build*/.ninja_log` lines (`start_ms end_ms mtime output
  cmdhash`, log format v5/v6). Duration = `end_ms - start_ms`. Classifies kind by output
  extension (`.o/.obj` → compile; `.elf/.a/.so/.bin/.hex` → link; else → other). Covers
  **west, cmake, idf.py** with no opt-in.
- `cargo_timings` — finds the newest `target*/cargo-timings/cargo-timing-*.html`,
  scrapes the embedded `UNIT_DATA` JSON array (`name`, `start`, `duration`, `rmeta_time`,
  `mode`). Build-script units (`build.rs`) map to the codegen stage; the rest to compile.
  Covers **cargo** (native, esp32 bare-metal, cross).

A collector that finds no artifacts returns empty (not an error) so a partial profile
still renders.

**2. Normalizer** — merges collector output into one value:

```
BuildProfile {
    backend: Backend,                 // Ninja{tool: West|Cmake|Idf} | Cargo | Mixed
    total_s: f64,
    stages: Vec<Stage>,               // {name: Codegen|Compile|Link|Flash, dur_s, pct}
    units:  Vec<Unit>,                // {name, kind, dur_s, backend}
    captured_deep: bool,              // false when only wall-clock/coarse available
}
```

Stage durations come from summing unit durations per kind; `total_s` from the span
(`max(end) - min(start)`). When deep data is absent (e.g. cargo without `--timings`),
`stages` carries a single coarse `Compile` span and `captured_deep = false`.

**3. Diagnostics** — a small, data-driven rule set over `BuildProfile`; each rule emits
zero or one hint string. v1 rules:

- **cold-C-build** — one C/link unit is a large fraction (>50%) of compile time → hint to
  enable a compiler cache for warm rebuilds.
- **shared-crate-recompiled-N×** — same crate name appears across multiple `target*/`
  dirs or repeats within a session → hint to pool `target_dir` (phase-226 pattern).
- **isolated-target-dir** — project builds into a per-example `target/` with no pooling →
  hint.
- **job-count-vs-RAM** — read `NROS_BUILD_JOBS` + `/proc/meminfo`; warn if the job count
  risks the issue #57 OOM, confirm if within budget.

Rules are independent and individually suppressible (`--no-hints`).

**4. Reporter** — renders:

- the **stage table** (always),
- a **`--deep` drill-down** (top-N slowest units with bars; a note when
  `captured_deep == false`),
- **hints** (default-on small set; `--no-hints` to silence),
- **`--json`** → writes `nros-build-profile.json` (backend, total, stages, units, hints)
  for CI regression diffing.

No heavy table deps; plain formatting with minimal color.

### `just profile` recipe

`just profile <example-or-dir> [--deep]` runs the analyzer against an **already-built**
project dir. It does not build (consistent with "no `nros` build/test"). For deep cargo
data, the in-repo build recipes inject `cargo build --timings` under `NROS_PROFILE=1`;
the analyzer detects missing cargo-timings and prints a one-line hint to re-run with
`--timings` rather than failing.

## Data flow example

```
$ west build -b qemu_cortex_a53 examples/zephyr/rust/talker   # build/.ninja_log written
$ just profile examples/zephyr/rust/talker
Backend: ninja (west)        Total: 41.2s
Stage      Duration    %
codegen      1.1s      3%
compile     33.8s     82%   ← bottleneck
link         6.0s     15%
hint: 1 unit = 61% of compile (libzenoh_pico.a, 20.6s, C, no incremental)
```

## Error handling

- No artifacts at all → exit non-zero with an actionable message naming where the tool
  looked (`build*/.ninja_log`, `target*/cargo-timings/`) and how to produce them.
- Partial artifacts → render what is available, mark `captured_deep = false`, hint at the
  missing half.
- Malformed `.ninja_log` / timings → skip the bad line/file, count skips, note in output
  (never abort the whole report on one bad row).

## Testing (no compilation at test time)

- **Parser unit tests** against checked-in fixture artifacts
  (`tests/fixtures/sample.ninja_log`, `tests/fixtures/cargo-timing.html`) → assert the
  normalized `BuildProfile` (stage sums, unit count, backend tag).
- **Diagnostics tests** — hand-built `BuildProfile` values exercising each rule on/off.
- **Reporter golden test** — fixed `BuildProfile` → expected table/JSON text.
- **One integration** — `just profile` against a prebuilt example fixture dir; assert the
  table names the expected stages. No live build is run by the test.

## Scope summary

**v1:** two collectors (`ninja_log`, `cargo_timings`), normalizer, 4 diagnostics rules,
reporter (table + `--deep` + `--json` + `--no-hints`), `just profile` recipe, standalone
bin, fixture-based tests.

**Deferred:** ETA, trend DB, web UI, sccache stats, flash-internal timing.
