# Phase 182 - test-all coverage de-duplication and matrix trim

**Goal.** Cut `just test-all` wall-clock by removing tests that duplicate
coverage already provided by `build-all` or by a sibling runtime test, merging
near-identical tests, and trimming over-parametrised matrices — **without
losing real coverage**. Plus one orthogonal lever: stabilise flaky E2E so the
retry budget stops tripling their cost.

**Status.** Proposed. Created 2026-05-26 from the clean-rebuild `test-all`
analysis (full `clean` → `build-all` → `test-all`: 978 tests, 339 s wall,
~5000 s CPU).

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

### 182.1 — Drop `phase_118_collapse` (173 build-only presence checks)

`tests/phase_118_collapse.rs` is 173 `rstest` cases that assert "the per-RMW
fixture binary exists with the expected name" (`build_native_*_rmw()` resolver).
It builds nothing itself; the binaries are produced by `build-all`
(Phase 181), their presence is gated by the `_require-fixtures` stamp, and the
`native_api` / `rtos_e2e` runtime tests consume them (failing loudly on a
missing/misnamed binary). The file's own header concedes "the pubsub / service /
action runtime smoke tests cover one RMW per scenario already." → **Drop the
file, or collapse to ~6 data-driven sanity rows** (one per role, not per
role×RMW). Saves ~56 s CPU + 173 from the count, zero coverage loss.
**Files**: `packages/testing/nros-tests/tests/phase_118_collapse.rs`.

### 182.2 — Merge the two clean-cmake configure smokes

`cmake_add_subdirectory::cmake_add_subdirectory_smoke` (Phase 137.4, 164 s) and
`cmake_platform_matrix::cmake_platform_posix` (Phase 138.6, 164 s) are
near-identical: both write a minimal user project (`NANO_ROS_PLATFORM=posix`,
`NANO_ROS_RMW=zenoh`), `add_subdirectory(<root>)`, link `NanoRos::NanoRos`, call
`nros_support_get_zero_initialized()`, then `cmake` configure + build the full
nros-c/cpp stack from a clean dir. The matrix header already states posix is
"the same shape as Phase 137's `cmake_add_subdirectory` smoke." → **Merge into
one test** holding the union of assertions (umbrella `NanoRos::NanoRos` target +
header/link-order regression checks + §A `nros_platform_link_app` dispatch).
Saves ~164 s CPU (and likely wall-clock — these slow clean configures serialize
against each other). **Files**: `tests/cmake_add_subdirectory.rs`,
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
- [ ] 182.1 + 182.2 landed (the safe, zero-coverage-loss wins): `phase_118_collapse`
  dropped/collapsed, the two cmake smokes merged. ~220 s CPU off, ~174 fewer tests.
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
