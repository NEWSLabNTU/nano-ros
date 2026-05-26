# Phase 182 - test-all coverage de-duplication and matrix trim

**Goal.** Cut `just test-all` wall-clock by removing tests that duplicate
coverage already provided by `build-all` or by a sibling runtime test, merging
near-identical tests, and trimming over-parametrised matrices — **without
losing real coverage**. Plus one orthogonal lever: stabilise flaky E2E so the
retry budget stops tripling their cost.

**Status.** In progress. Created 2026-05-26 from the clean-rebuild `test-all`
analysis (full `clean` → `build-all` → `test-all`: 978 tests, 339 s wall,
~5000 s CPU). **182.1 + 182.2 landed** (the safe zero-coverage-loss wins): −165
build-only presence cases from `phase_118_collapse` (kept its 8 real Cyclone
e2e tests) and the two duplicate clean-cmake smokes merged into one. Net ~167
fewer tests + one fewer ~160 s cmake configure. 182.3–182.6 open.

**Priority.** P2 (developer + CI wall-clock).

**Depends on / Related.**
- **Phase 179** (nextest runtime profiling) — complementary: 179 removes serial
  waits / hidden runtime builds / over-broad serialization; **182 removes
  redundant *tests*** (coverage de-dup). Land either order.
- **Phase 181** (fixture build SSOT) — `build-all` now builds every fixture from
  `examples/fixtures.toml`, which is what makes the build-only test cells
  redundant.
- **Phase 177 / G6** — the retry-budget fix for `xrce` (`retries = 2`) is the
  template for the orthogonal lever below.

## Overview

The `test-all` run is one large `cargo nextest` stage (978 tests) plus doctests
/ Miri / C codegen. The nextest stage is ~5000 s of CPU collapsed to ~339 s wall
by per-group `max-threads` parallelism, so wall-clock is gated by the
longest-pole serial chains inside the most-constrained groups, and inflated by
flaky-test retries (`retries = 2` triples a flaky test's cost).

**Inventory by type (last-attempt CPU):**

| category | count | CPU | nature |
|----------|------:|----:|--------|
| E2E-runtime (pubsub/service/action/interop) | 104 | 2887 s | the product + the cost |
| OTHER / unit / lint | 317 | 780 s | incl. `phase_118_collapse` (173) |
| BOOT-smoke (`_starts` / `_boots`) | 71 | 399 s | "does the fixture boot" |
| BUILD-only (`_builds`) | 54 | 70 s | "does the fixture compile" |

**Wall-clock long poles:** `rtos_e2e` (4799 s CPU incl. retries, max 227 s per
test), the two ~164 s clean-cmake configs, `zephyr` action (113 s / 91 s),
`params` ROS 2 (69 s each).

## Work Items

### 182.1 — Drop the `phase_118_collapse` build-only presence matrices — DONE

**Scope correction (found on implementation):** `phase_118_collapse.rs` was
**not** a clean 173-build-only-checks file as first assumed — it was *mixed*:
~16 `*_rmw_variant_exists` `rstest` matrices (~165 build-only presence/name
cases) **plus 8 real Phase 11W Zephyr Cyclone DDS native_sim runtime E2E +
boot tests** (`test_zephyr_*_cyclonedds_{pubsub,service}_e2e` / `_boot`) that
spawn `ZephyrProcess` and assert delivered samples / replies (up to 33 s each;
all PASS). Blindly "dropping the file" would have deleted real coverage, and a
`.config/nextest.toml` override (`binary(phase_118_collapse) and test(cyclonedds)
…`) routes those e2e tests.

**Done:** removed the ~16 presence matrices + the dead `require_cmd`-style
helper + dead imports; **kept the 8 Cyclone e2e tests** in place (the binary
name is retained so the nextest override still matches). Build-only coverage is
provided by `build-all` (Phase 181) + the runtime tests that consume the
binaries. Result: `phase_118_collapse` went **173 → 8 tests** (−165), zero
runtime-coverage loss. Verified: compiles with no warnings, all 8 e2e tests
list. Follow-up (optional): rename the file to `zephyr_cyclonedds_e2e.rs` and
update the nextest filter — deferred to avoid the config churn now.
**Files**: `packages/testing/nros-tests/tests/phase_118_collapse.rs`.

### 182.2 — Merge the two clean-cmake configure smokes — DONE

`cmake_add_subdirectory::cmake_add_subdirectory_smoke` (Phase 137.4) and
`cmake_platform_matrix::cmake_platform_posix` (Phase 138.6) were near-identical:
both wrote a minimal user project (`NANO_ROS_PLATFORM=posix`,
`NANO_ROS_RMW=zenoh`), `add_subdirectory(<root>)`, linked `NanoRos::NanoRos`,
called `nros_support_get_zero_initialized()`, then `cmake` configure + build the
full nros-c/cpp stack from a clean dir.

**Done:** added the §A dispatch assertion (`nros_platform_link_app(smoke)`) to
the surviving `cmake_add_subdirectory_smoke` template, then deleted
`cmake_platform_posix` + its now-dead `run_platform_cell` helper from the matrix
file. `cmake_platform_matrix` keeps only its non-overlapping
`cmake_platform_threadx_requires_board` FATAL_ERROR check. Verified: merged test
PASSES (configure + build + `nros_platform_link_app`), one fewer ~160 s clean
cmake configure. **Files**: `tests/cmake_add_subdirectory.rs`,
`tests/cmake_platform_matrix.rs`.

### 182.3 — Drop or relocate `_builds` cells (54, ~70 s)

The `*_builds` tests assert a fixture compiles — exactly what `build-all` does
for every fixture (Phase 181). A `build-all` failure already surfaces a broken
fixture before `test-all` runs (the `_require-fixtures` preflight). → **Drop the
pure-compile assertions from the nextest stage**, or gate them behind a
`build`-tier filter that `test-all` skips. Keep any `_builds` that compile a
*configuration not covered by `build-all`* (audit first). **Files**: per-binary
(`emulator`, `zephyr`, `esp32_emulator`, `c_xrce_api`, …).

### 182.4 — Audit redundant BOOT-smoke (`_starts` / `_boots`, 71, ~399 s)

Where an `_e2e` test exists for the same fixture, it already boots that binary
and does more, so the sibling `_starts`/`_boots` is redundant. → **Audit each
`_starts`/`_boots`; drop the ones whose fixture is already booted by an `_e2e`.**
Keep boot-smokes for fixtures with *no* e2e counterpart (bring-up-only boards).
**Files**: `emulator`, `zephyr`, `esp32_emulator`, `xrce`, `freertos_qemu`.

### 182.5 — Trim the `rtos_e2e` matrix (the wall-clock critical path)

`rtos_e2e` = 4 platforms × {pubsub, service, action} × {Rust, C, Cpp} = 36 base
combos, ×`retries = 2` = the 4799 s CPU critical path. The three language
bindings exercise the *same wire path* per (platform, scenario). Proposal:
- keep **all langs for pubsub** (cheapest scenario; proves transport + each
  binding end to end);
- trim **action** (slowest + flakiest scenario — where the NuttX/Cpp hang of
  **177.30** lives) to **Rust + one of {C, Cpp} per platform**;
- keep **service** as-is or trim symmetrically.

Biggest single wall-clock win. **Risk:** a language-specific *action* regression
could slip if its lang cell is dropped — this is a coverage-vs-speed judgment for
the maintainer, not a safe mechanical change. **Files**: `tests/rtos_e2e.rs`
(`#[case]` matrix), `.config/nextest.toml` (group sizing).

### 182.6 — Orthogonal lever: stabilise flaky E2E to kill retry inflation

`retries = 2` triples a flaky test's CPU cost and can extend the critical path
(a flake forces a serial re-run inside a `max-threads`-capped group). The
2026-05-26 run had **26 flaky**. Each stabilised test reclaims up to 2× its
runtime. Pattern (from Phase 177 / G6): root-cause the flake — usually an
in-test `wait_for_output_pattern` timing out under host saturation, a fixed
`sleep(N)` stabilisation (CLAUDE.md says replace with readiness waits), or
`.unwrap_or_default()` masking a timeout — then fix the readiness wait rather
than leaning on the retry. Targets: the flaky members of `rtos_e2e`, `zephyr`,
`emulator`, `large_msg`. Retries stay as a safety net, but should not be the
routine path. **Files**: the flaky tests' bodies + their fixtures'
readiness markers.

## Acceptance

- [ ] `just test-all` runs fewer tests with **no loss of real coverage** —
  every dropped test's path is provably covered by `build-all` or a sibling
  `_e2e` (documented per drop).
- [x] 182.1 + 182.2 landed (the safe, zero-coverage-loss wins): `phase_118_collapse`
  trimmed 173 → 8 (build-only presence matrices removed, 8 Cyclone e2e kept),
  the two cmake smokes merged into one. ~167 fewer tests + one fewer ~160 s
  clean cmake configure. Verified: compiles clean, merged cmake test passes.
- [ ] Flaky count trends down (182.6); retry budget is a net, not the norm.
- [ ] `rtos_e2e` matrix decision recorded (trim or keep, with rationale) — 182.5.
- [ ] `examples/README.md` coverage matrix still agrees with the surviving tests.

## Notes

- **Safe-now vs judgment.** 182.1 (drop presence-checks) and 182.2 (merge cmake
  smokes) are mechanical, zero-coverage-loss — land them first. 182.3 / 182.4 need
  a per-test audit ("is this path covered elsewhere?"). 182.5 is a deliberate
  coverage-vs-speed trade for the maintainer.
- **Don't confuse with Phase 179.** 179 makes the *same* test set faster (serial
  waits, hidden builds, group sizing). 182 makes the test *set smaller*. They
  compound.
- The CPU numbers are last-attempt sums from the 2026-05-26 run; wall-clock
  impact of each item depends on whether the test sits on a constrained group's
  serial chain — measure with the Phase 179 profiling harness after each change.
