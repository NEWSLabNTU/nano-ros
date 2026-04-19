# Phase 85: Test-Suite Consolidation & Speedup

**Goal**: Reduce the 214-function integration-test matrix in
`packages/testing/nros-tests/tests/` to a single-source-of-truth layout
that's ~70% smaller and ~40% faster to run, without sacrificing any
platform coverage.

**Status**: Not Started
**Priority**: Medium — the suite isn't broken, but the duplication is
compounding: every new platform / transport has to be added in three
places (build helper, build test, E2E test), every bug fix that changes
a sleep-based wait has to be replicated across four platform files, and
the per-file `OnceCell` caches rebuild the same cross-compiled binaries
multiple times per nextest run.
**Depends on**: None — can land in parallel with Phase 80 and Phase 84.

## Overview

### Current state (audited 2026-04-20)

The integration-test suite under
`packages/testing/nros-tests/tests/` has accumulated substantial
duplication as platforms were added one at a time. A detailed audit
found:

| Surface                             | Count | Notes                                                        |
|-------------------------------------|:-----:|--------------------------------------------------------------|
| Test functions                      |  214  | Across 30 `.rs` files                                        |
| Build-only tests (`test_X_builds`)  |  69   | One per (platform, example, language) tuple                  |
| Near-identical E2E test bodies      |  32   | Same assertions, different `QemuProcess::start_X_virt`       |
| Per-file `OnceCell<PathBuf>` caches |  30   | No cross-file sharing — same binary rebuilt per test file    |
| `sleep(Duration::from_secs(≥2))`    |  11+  | Mostly in c_api / cpp_api / safety_e2e; ~15–30s per run      |
| Nextest `max-threads=1` groups      |   8   | One per platform; mostly over-restrictive post-port-table    |
| `#[ignore]`'d tests                 |   2   | Both genuinely blocked (Phase 77 C++ action; XRCE forwarder) |

### What "the same test, 32 times" looks like

Taking `test_freertos_pubsub_e2e` and `test_nuttx_c_pubsub_e2e` side by
side: both launch two QEMU instances, both sleep ~10 s for
stabilization, both scrape stdout for a `"Received"` pattern, both kill
processes at the end. The *only* difference is the `start_freertos_virt`
vs. `start_nuttx_virt` call and which per-platform zenohd port they
open. Same shape repeats for:

- FreeRTOS: C / Rust / C++ × {pubsub, service, action} = 9 tests
- NuttX:    C / Rust / C++ × {pubsub, service, action} = 9 tests (3 C++ `#[ignore]`d upstream)
- ThreadX Linux: C / Rust / C++ × {pubsub, service, action} = 8 tests
- ThreadX RISC-V: C / Rust / C++ × {pubsub, service, action} = 7 tests

That's the 32-body cluster. Build-only tests have the same problem at
higher arity (4 platforms × ~5 examples × 3 languages = 60-ish tests
that each just call `build_X()` and assert the binary exists).

### Non-goals

- **No test deletion that reduces coverage.** Every platform / language
  combination currently tested stays covered.
- **No change to the `#[ignore]`'d tests' blockers** (Phase 77 async
  action client, XRCE forwarder). Phase 85 doesn't touch the reasons,
  just the test plumbing.
- **No touching the rustfmt / clippy / miri / doctest pipelines** —
  those already run cleanly and are orthogonal to the integration-test
  redundancy.

## Work Items

### Group A — Land today (quick wins, ~0 risk)

- [x] 85.1 — Merge `c_api.rs` + `cpp_api.rs` into `native_api.rs`
  - **Files**: `packages/testing/nros-tests/tests/c_api.rs`,
    `packages/testing/nros-tests/tests/cpp_api.rs` → new
    `packages/testing/nros-tests/tests/native_api.rs`
  - **Goal**: Consolidate with `#[rstest(language in ["c", "cpp"])]`.
    Both files have identical assertion shapes on different binaries;
    the duplication is mechanical. Saves ~100 LOC + one test binary.
  - **Coverage**: Every existing native C / C++ test scenario runs
    under the parametrised test with the same assertions.

- [x] 85.2 — Replace `sleep(Duration::from_secs(N))` with ready-probes
  - **Files**: `c_api.rs`, `cpp_api.rs`, `safety_e2e.rs` (and any other
    files surfaced by `rg 'sleep\(Duration::from_secs\([2-9]'`)
  - **Goal**: Swap stabilization sleeps for
    `nros_tests::wait_for_port(port, Duration)` or equivalent. zenohd
    binds before `ZenohRouter::start()` returns, so the sleep is a
    belt-and-suspenders wait for something that's already finished.
  - **Expected speedup**: 15–30 s per full run. More if we push the
    probe into `spawn` / `wait_for_output_pattern` helpers.

- [x] 85.3 — Delete the 60+ build-only `test_X_Y_builds` tests
  - **Files**: `freertos_qemu.rs`, `nuttx_qemu.rs`, `threadx_linux.rs`,
    `threadx_riscv64_qemu.rs`
  - **Goal**: Remove `test_X_talker_builds`, `test_X_listener_builds`,
    `test_X_service_*_builds`, `test_X_action_*_builds` functions.
    Keep one per-platform `test_X_detection` smoke test (already
    exists: `test_threadx_detection`, etc.) and the `#[ignore]`d NuttX
    C++ build tests (those *are* the feature gate for the upstream
    libc block).
  - **Coverage rationale**: Every E2E test invokes the same
    `build_X_example()` helper first; if the build fails, the E2E test
    fails before `sleep(10)`. A dedicated build-only test adds no
    signal for ~20 s of wasted build time per test function.

### Group B — Next PR (medium risk, touches test file structure)

- [ ] 85.4 — Parametrise the E2E tests by (platform, variant, language)
  - **Files**: `freertos_qemu.rs`, `nuttx_qemu.rs`, `threadx_linux.rs`,
    `threadx_riscv64_qemu.rs` → new
    `packages/testing/nros-tests/tests/rtos_e2e.rs`
  - **Goal**: Collapse the 32-body E2E cluster into three parametrised
    test functions:
    ```rust
    #[rstest]
    fn test_rtos_pubsub_e2e(
        #[values(FREERTOS, NUTTX, THREADX_LINUX, THREADX_RV64)] platform: Platform,
        #[values(Lang::Rust, Lang::C, Lang::Cpp)] lang: Lang,
    ) { ... }
    // + _service_e2e, _action_e2e
    ```
    The platform-specific bits
    (`QemuProcess::start_freertos_virt` / `_nuttx_virt` / etc.) move
    behind a `Platform::start(binary)` method. The zenohd port comes
    from the existing `nros_tests::platform::*` table.
  - **Per-platform skips**: `#[ignore]` or `platform.supports(lang)`
    check where a platform doesn't build that language — e.g.,
    FreeRTOS has no XRCE C tests.
  - **Benefit**: One place to change the ready-probe, the timeout, the
    output pattern. Adding a new platform becomes a one-line
    `Platform` table entry instead of a new `*.rs` file.

- [ ] 85.5 — Consolidate `OnceCell<PathBuf>` builders into a shared module
  - **Files**: new
    `packages/testing/nros-tests/src/fixtures/binaries.rs` module
    (the existing `fixtures/binaries.rs` grows a per-platform
    submodule), the per-platform `.rs` test files shed their static
    blocks.
  - **Goal**: One `OnceCell<PathBuf>` per (platform, example,
    language) triple, visible to every test binary in a nextest run.
    Right now `freertos_qemu.rs` and `emulator.rs` both rebuild the
    same FreeRTOS-QEMU binaries because their caches are independent.
  - **Nextest note**: nextest runs each `.rs` test binary in a
    separate process, so statics don't cross the binary boundary
    anyway. The win is *within* the rtos_e2e.rs file from 85.4: one
    build serves all (variant × language) test runs for a given
    platform.

- [ ] 85.6 — Shrink the nextest `max-threads=1` group list
  - **Files**: `.config/nextest.toml`
  - **Goal**: Replace the 8 per-platform groups with two groups:
    - `qemu-serial` — everything that launches a QEMU instance and
      needs `max-threads = 1` (avoids virtio-net TAP/slirp contention)
    - default — native / POSIX / interop tests, parallel
  - The per-platform zenohd-port table (`nros_tests::platform`) and
    the per-test `ZenohRouter::start(port)` already solve the
    zenohd-port-collision problem that motivated the original
    per-platform groups; the only remaining reason for serialising is
    QEMU resource pressure on a single host, which one group captures.

### Group C — Design discussion (bigger, optional)

- [ ] 85.7 — Recipe rename pass
  - **Files**: `justfile`, `platforms/*.just`
  - **Goal**: Rename `just test` → `just test-fast` (host / POSIX
    only), `just test-all` → `just test` (the default that also
    covers QEMU platforms). `just ci` uses `test-fast`; nightlies
    use `test`. Removes the ad-hoc `--exclude` list in the current
    `just test` recipe — the scope is now declared by group
    membership in nextest.toml, not by a hand-maintained exclusion.
  - **Caveat**: This is a UX break for anyone who has `just test` in
    muscle memory. Pair the rename with a
    `just test: @echo "renamed to test-fast"; just test-fast` alias
    during a transition window.

- [ ] 85.8 — Port-table migration for non-RTOS tests
  - **Files**: `packages/testing/nros-tests/src/platform.rs` (the
    `platform::NATIVE` / `platform::NATIVE_XRCE` entries if they
    don't already exist), `c_api.rs` / `cpp_api.rs` / `services.rs` /
    `actions.rs`
  - **Goal**: Replace any hardcoded zenohd ports in native tests with
    `platform::NATIVE.zenohd_port`. Removes a footgun where two
    parallel tests on the same dev box pick the same port. Pairs
    naturally with 85.6 (reducing the need for `max-threads = 1`).

## Acceptance Criteria

- [ ] `just test` (post-85.7 rename: `just test-fast`) completes in at
      least 15 % less wall time than the current baseline on a clean
      checkout.
- [ ] `just test-all` (post-rename: `just test`) completes in at least
      30 % less wall time than the current baseline.
- [ ] Total integration test function count drops from 214 to under 100
      (Group A alone should bring it to ~140; Group B to ~80).
- [ ] No platform coverage lost: every (platform, language, variant)
      combination currently tested still has at least one passing
      assertion in the new suite.
- [ ] `rg 'sleep\(Duration::from_secs\([2-9]' packages/testing/nros-tests/`
      returns zero results outside of intentional pacing in long-lived
      E2E fixtures.
- [ ] Single source of truth: adding a new RTOS platform is a one-line
      entry in `nros_tests::platform` + one `start_X_virt` fixture
      helper, not a new test `.rs` file.

## Notes & Caveats

- **Don't break the Phase 77 / Phase 83 regression coverage.** The
  `test_cpp_action_communication`, `test_cpp_action_goal_rejection`,
  `test_nuttx_c_{pubsub,service,action}_e2e` tests are the load-bearing
  regression guards for recently-landed phases. Each must still be a
  discrete, nameable assertion — parametrisation is fine, outright
  deletion isn't.
- **Don't rely on "the E2E test covers the build path" for
  toolchain-gate tests.** The NuttX C++ `test_X_builds` functions are
  `#[ignore]`d as a declarative marker for the upstream
  `_SC_HOST_NAME_MAX` issue; the E2E test wouldn't run either way.
  Keep those markers.
- **Don't over-parametrise.** A `#[rstest]` matrix that spans 4
  platforms × 3 languages × 3 variants = 36 test cases is fine, but if
  a single case has per-platform quirks (like NuttX's C service test
  needing `#[ignore]`), split the matrix rather than hiding the
  `if platform == NUTTX { ... }` branch inside the test body.
- **Baseline the timings before 85.1 lands.** Capture the current
  `just test` / `just test-all` wall time on reference hardware;
  the acceptance criteria above are percentages, not absolute targets,
  and the percentages need a baseline to check against.

## How to run the audit again later

```
# Enumerate test functions per file
rg -n '^(pub )?fn test_' packages/testing/nros-tests/tests/ | wc -l

# Find slow sleeps
rg -n 'sleep\(Duration::from_secs\([2-9]' packages/testing/nros-tests/

# Spot per-file OnceCell caches (drift indicator)
rg -n 'static [A-Z_]+_BINARY: OnceCell<PathBuf>' packages/testing/nros-tests/
```

Re-run after every phase closure: each phase that adds a new platform
or example should leave these counts stable or smaller, never larger.
