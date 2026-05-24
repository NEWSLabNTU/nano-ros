# Phase 179 - test-all wall-clock profiling and fixups

**Goal.** Turn `just test-all` from an opaque hour-long full sweep into a
measured pipeline, then remove avoidable serial waits, hidden builds, and
over-broad stage composition without reducing coverage.

**Status.** Proposed. Created from the 2026-05-24 `just test-all`
review.

**Priority.** P2 (developer and CI wall-clock).

**Depends on.** Phase 178 for fixture/build de-dup. This phase starts
with measurement because the current runtime is spread across nextest,
doctests, Miri, C codegen, orchestration E2E, ROS 2 CLI work, and
platform runtime tests.

## Findings

### test-all is a serial aggregate

`just test-all` currently runs these stages in order:

- `build-zenohd`
- `cargo nextest run --workspace --no-fail-fast`
- `_test-summary`
- `test-doc`
- `test-miri`
- `native _test-c-codegen`
- `native _test-orchestration-e2e`

Fixtures are intentionally not built by this recipe; callers are
expected to run `just build-test-fixtures` first. If a measured "test
all" run includes fixture staging, the build side belongs to Phase 178.
This phase tracks the test runner side.

### nextest contains most platform runtime work

The workspace nextest run includes the broad E2E matrix: Zephyr,
FreeRTOS, NuttX, ThreadX, XRCE, ROS 2 interop, native C/C++ API, bridges,
large messages, zero-copy, safety, and other integration binaries. The
single nextest invocation is convenient, but its slowest tests are not
surfaced in the top-level `test-all` output beyond the JUnit file.

### Several test groups are deliberately serialized

`.config/nextest.toml` caps multiple groups at `max-threads = 1`:

- Zephyr native_sim fall-through tests, due historical parallel CMake /
  Kconfig corruption under I/O pressure.
- Zephyr CycloneDDS, due fixed DDS discovery port/domain use.
- XRCE and XRCE-to-ROS 2 interop.
- ROS 2 interop, due ROS 2 CLI and discovery contention.
- Native C/C++ API tests, because variants share
  `target/release/libnros_c.a`.
- ESP32 emulator tests.

Some of these caps are still necessary. Others may now be overly
conservative after fixture prebuild and per-language port work, but they
need measurements and isolation audits before tuning.

### Fixed sleeps create guaranteed dead time

Several E2E tests wait with fixed sleeps before checking readiness or
collecting output. Examples observed during review include C XRCE API,
custom transport loopback, zero-copy, safety E2E, and ROS 2 lifecycle
interop tests. These waits accumulate even when the process was ready
earlier.

### Post-nextest stages have poor visibility

Doctests, Miri, C codegen, and orchestration E2E run after nextest and
outside nextest scheduling. The top-level summary is printed before
those later stages, so a slow or failing late stage is not reflected in
the same timing/reporting surface as the workspace tests.

### Hidden builds may still exist inside tests

The full test suite should consume artifacts from
`just build-test-fixtures` where practical. Any remaining test-body
builds make runtime unpredictable, increase target-dir contention, and
hide build regressions inside `test-all` instead of Phase 178's fixture
stage.

## Plan

- [ ] **179.A - add a profiling script for test-all.** Add a small
  script, for example `scripts/test/profile-test-all.sh`, that runs the
  same stages as `just test-all` with per-stage timers, captures stdout
  and stderr under `tmp/test-all-*/`, records command lines, exit codes,
  host/core information, and points `tmp/test-all-latest` at the newest
  run. The script should support a dry-run or command-print mode so CI
  logs show exactly what will run.

- [ ] **179.B - wire stage timing into just test-all.** Either call the
  profiling script from the recipe or share a helper so normal
  `just test-all` prints durations for `build-zenohd`, nextest,
  doctests, Miri, C codegen, and orchestration E2E. Keep the current
  pass/fail behavior.

- [ ] **179.C - report slowest nextest tests.** After nextest completes,
  parse `target/nextest/default/junit.xml` and print the slowest tests
  with binary, test name, duration, retry count when available, and
  group if it can be recovered from nextest metadata. This makes the
  long pole visible without opening XML by hand.

- [ ] **179.D - include late stages in the final summary.** Extend the
  summary output so doctests, Miri, C codegen, and orchestration E2E
  appear with status and duration beside the nextest result. The final
  failure message should identify the failing stage.

- [ ] **179.E - audit and remove fixed sleeps.** Replace fixed sleeps in
  E2E tests with readiness polling, log-pattern waits, port-open waits,
  or first-message deadlines. Keep upper bounds so failures still time
  out clearly. Start with C XRCE API, custom transport loopback,
  zero-copy, safety E2E, and ROS 2 lifecycle interop.

- [x] **179.F - find remaining test-body builds.** Add a review pass for
  helpers named like `build_*` or tests that call cargo, CMake, west,
  make, or platform build scripts during `test-all`. Move expensive
  required artifacts into `build-test-fixtures`, or document why the
  build must stay inside the test.

  Completed 2026-05-25. Added `scripts/test-audit-builds.sh` as the
  repeatable review pass. It reports direct build-tool spawns, shell
  command strings mentioning build tools, and `build_*` fixture resolver
  call sites that can be mistaken for in-test compiles.

  Moved one avoidable build out of the test body:
  `zpico_build_matrix::zpico_posix_archive_carries_link_feature_symbols`
  now consumes the deterministic POSIX staticlib staged by
  `just build-test-fixtures` / `just build-zenoh-posix-fixture` at
  `target-zenoh-fixture-posix/` instead of running its own
  `cargo build -p nros-rmw-zenoh-staticlib`.

  Remaining direct build-tool invocations are intentional:

  - `zpico_build_matrix::zpico_sys_has_no_cmake_dep` runs `cargo tree`;
    this is metadata inspection, not a compile.
  - `zpico_drift_gate` runs sandboxed `cargo build -p zpico-sys` twice
    because the build-script failure/success path is the product under
    test.
  - `cmake_add_subdirectory` and `cmake_platform_matrix` configure and
    build tiny throwaway consumers because CMake source-distribution
    linkability is the product under test.
  - `integration_zephyr` uses `west list` and `integration_esp_idf`
    uses `idf.py --version`; both are setup probes, not builds.
  - `integration_px4` uses `make help` and `px4_e2e` builds PX4 SITL,
    but `px4_e2e` is behind the non-default `px4-sitl` feature and is
    not part of the default `cargo nextest run --workspace` in
    `just test-all`.
  - `nros-cli-core` orchestration E2E tests invoke `nros build` /
    `build::run` and compile small native counter archives because they
    validate the build command and generated-package link behavior
    directly. These remain outside `nros-tests` fixture staging.
  - `build_*` helpers under
    `packages/testing/nros-tests/src/fixtures/binaries/` are fixture
    resolvers by contract; they should only return prebuilt paths or a
    missing-fixture remedy. Any future cargo/CMake/west/make command in
    those helpers is a 179.F regression.

- [ ] **179.G - split shared native C/C++ artifacts.** Native API tests
  serialize because zenoh and XRCE variants share
  `target/release/libnros_c.a`. Move those tests to per-RMW target dirs
  or fixture archives so they can run concurrently and stop overwriting
  each other.

- [ ] **179.H - re-evaluate Zephyr test serialization.** Confirm which
  Zephyr tests still configure/build inside the test body. Runtime-only
  tests that consume prebuilt images and use unique ports may be able to
  leave the historical `qemu-zephyr max-threads = 1` bottleneck without
  reintroducing the old CMake corruption.

- [ ] **179.I - isolate ROS 2 and XRCE interop enough to parallelize.**
  Survey use of ROS domain IDs, daemon behavior, DDS discovery ports,
  XRCE Agent ports, and temp dirs. Where tests can own unique domains
  and ports, split them out of the global serialized groups.

- [ ] **179.J - parallelize post-nextest stages safely.** After stage
  timing exists, run doctests, Miri, C codegen, and orchestration E2E in
  parallel when their target dirs and external services do not collide.
  Keep per-stage logs and a combined exit status.

- [ ] **179.K - add focused full-suite lanes.** Keep `just test-all` as
  the exhaustive local/CI gate, but add documented lanes such as
  `test-all-runtime`, `test-all-ros2`, `test-all-miri`, and
  `test-all-codegen` if profiling shows developers repeatedly need only
  one slow slice.

- [ ] **179.L - add a fast-fail variant.** Preserve the current
  `--no-fail-fast` full report behavior, but provide an opt-in
  fail-fast recipe or environment knob for local diagnosis when a slow
  platform is already known broken.

## Acceptance

- `just test-all` prints per-stage durations and points to a stable log
  directory for the run.
- The slowest nextest tests are visible in normal output.
- Doctests, Miri, C codegen, and orchestration E2E are represented in
  the final status summary.
- A first profiling run identifies the top long-pole tests and stages
  without manual XML/log digging.
- Fixed sleeps that are not semantically required are replaced by
  readiness waits with explicit deadlines.
- Remaining test-body builds are either moved to fixture staging or
  documented as intentional.
- Any raised nextest concurrency cap is backed by port/domain/build-dir
  isolation notes and a before/after timing comparison.
