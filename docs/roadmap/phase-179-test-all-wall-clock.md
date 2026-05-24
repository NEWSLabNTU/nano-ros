# Phase 179 - nextest runtime profiling and fixups

**Goal.** Make the `cargo nextest run` portion of `just test-all`
measurable and replayable, then remove avoidable serial waits, hidden
runtime builds, and over-broad serialization without reducing coverage.

**Status.** Proposed. Created from the 2026-05-24 `just test-all`
review.

**Priority.** P2 (developer and CI wall-clock).

**Depends on.** Phase 178 for fixture/build de-dup. This phase covers
only the nextest run. Doctests, Miri, C codegen, orchestration E2E, and
other outer `just test-all` stages are intentionally out of scope unless
they are moved under nextest later.

## Findings

### test-all has one large nextest stage

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
This phase tracks the nextest stage only.

### nextest contains most platform runtime work

The workspace nextest run includes the broad E2E matrix: Zephyr,
FreeRTOS, NuttX, ThreadX, XRCE, ROS 2 interop, native C/C++ API, bridges,
large messages, zero-copy, safety, and other integration binaries. The
single nextest invocation is convenient, but its slowest tests are not
surfaced in the top-level `test-all` output beyond the JUnit file.

### nextest already has the profiling primitives

Do not build custom per-test timing. Nextest already provides the
needed data:

- `target/nextest/default/junit.xml` records per-test `time` values and
  the whole nextest run time.
- `--status-level` and `--final-status-level` can surface slow tests in
  normal output.
- Experimental run recording (`NEXTEST_EXPERIMENTAL_RECORD=1`) captures
  a full event stream and captured stdout/stderr for every test.
- Recorded runs can be replayed, exported as portable archives, and
  exported as Chrome/Perfetto traces.
- Perfetto trace export includes test begin/end timestamps, global slot
  assignment, binary id, test name, command line, result, duration,
  attempt count, `is_slow`, and `test_group`.

The nextest trace covers the nextest test execution phase only. That is
acceptable for this phase because nextest timing is the requested scope.

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

### Hidden builds may still exist inside tests

The full test suite should consume artifacts from
`just build-test-fixtures` where practical. Any remaining test-body
builds make nextest runtime unpredictable, increase target-dir
contention, and hide build regressions inside the nextest stage instead
of Phase 178's fixture stage.

## Plan

- [x] **179.A - add nextest slow-test reporting.** Parse
  `target/nextest/default/junit.xml` after the nextest run and print the
  slowest tests with binary, test name, duration, and status. Keep this
  lightweight and available for normal `just test` / `just test-all`
  output.
  Landed as `scripts/test/nextest-slow-tests.py` plus the private
  `just _nextest-slow-tests` helper, called by `just test` and
  `just test-all` after `_test-summary`.

- [ ] **179.B - add an opt-in nextest profile recipe.** Add a recipe
  such as `just test-all-nextest-profile` or an environment knob that
  runs the same nextest command with `NEXTEST_EXPERIMENTAL_RECORD=1`,
  preserves normal nextest parallelism, and writes profiling artifacts
  under a timestamped `tmp/nextest-profile-*/` directory.

- [ ] **179.C - export replayable nextest logs.** For profiled runs,
  export the latest recording with `cargo nextest store export latest`.
  Keep the archive path stable via `tmp/nextest-profile-latest/` so a
  failing run can be replayed with full captured output, including
  successful test output when needed.

- [ ] **179.D - export Perfetto timeline traces.** For profiled runs,
  export `cargo nextest store export-chrome-trace latest --group-by slot`
  to a JSON trace. This is the canonical artifact for concurrency,
  group bottlenecks, idle slots, retries, and long-pole visualization.

- [ ] **179.E - document profiling overhead and retention.** Recording
  adds event/output-store writes and archive export can create sizable
  artifacts on chatty tests. Keep recording opt-in for local runs, avoid
  `--no-capture` because it serializes execution, and document how to
  prune the nextest store.

- [x] **179.F - find remaining test-body builds.** Add a review pass for
  helpers named like `build_*` or tests that call cargo, CMake, west,
  make, or platform build scripts during nextest. Move expensive
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

- [ ] **179.G - audit and remove fixed sleeps.** Replace fixed sleeps in
  E2E tests with readiness polling, log-pattern waits, port-open waits,
  or first-message deadlines. Keep upper bounds so failures still time
  out clearly. Start with C XRCE API, custom transport loopback,
  zero-copy, safety E2E, and ROS 2 lifecycle interop.

- [x] **179.H - split shared native C/C++ artifacts.** Native API tests
  serialize because zenoh and XRCE variants share
  `target/release/libnros_c.a`. Move those tests to per-RMW target dirs
  or fixture archives so they can run concurrently and stop overwriting
  each other.

  Completed 2026-05-25. The native C/C++ resolver path already consumes
  prebuilt per-RMW CMake fixture directories:
  `examples/native/{c,cpp}/<case>/build-zenoh`,
  `build-xrce`, and `build-cyclonedds`. The remaining blocker was stale
  test infrastructure: `.config/nextest.toml` still serialized
  `native_api` and `c_xrce_api` because older test helpers could rebuild
  shared `target/release/libnros_c.a` in the test body. Removed the
  `native_api` single-thread group assignment, documented that
  `c_xrce_api` uses prebuilt `build-xrce` fixtures plus per-test
  ephemeral XRCE Agent ports, fixed the C XRCE tests to pass those
  per-test locators through the `NROS_LOCATOR` environment variable that
  the examples actually read, and deleted dead C/C++ helper paths that
  still contained direct CMake builds for retired DDS examples.

  Follow-up race cleanup fixed the remaining unfiltered failures:
  `nros_cmake_configure_if_needed` now rejects half-configured CMake
  build dirs with no `Makefile`/`build.ninja`, `NanoRos` explicitly
  depends on the CMake CycloneDDS backend target when the Linux/BSD
  whole-archive linker flag path uses `$<TARGET_FILE:...>`, CycloneDDS
  runtime tests set `LD_LIBRARY_PATH` to the local `build/install/lib`,
  and the old ignored C service/action cases were unignored after
  passing. A parallel nextest run over `native_api`, `c_xrce_api`, and
  `cpp_parameters` now passes unfiltered with 41 tests run, 41 passed,
  and 0 skipped. The native C/C++ API tests can now use nextest's
  default scheduler.

- [ ] **179.I - re-evaluate Zephyr test serialization.** Confirm which
  Zephyr tests still configure/build inside the test body. Runtime-only
  tests that consume prebuilt images and use unique ports may be able to
  leave the historical `qemu-zephyr max-threads = 1` bottleneck without
  reintroducing the old CMake corruption.

- [ ] **179.J - isolate ROS 2 and XRCE interop enough to parallelize.**
  Survey use of ROS domain IDs, daemon behavior, DDS discovery ports,
  XRCE Agent ports, and temp dirs. Where tests can own unique domains
  and ports, split them out of the global serialized groups.

- [ ] **179.K - add focused nextest lanes.** Keep full nextest coverage
  available, but add documented filterset lanes such as runtime-only,
  ROS 2 interop, Zephyr, RTOS, or native API if profiling shows
  developers repeatedly need only one slow slice.

- [ ] **179.L - add a nextest fast-fail variant.** Preserve the current
  `--no-fail-fast` full report behavior, but provide an opt-in
  fail-fast recipe or environment knob for local diagnosis when a slow
  platform is already known broken.

## Acceptance

- The slowest nextest tests are visible in normal output from JUnit
  parsing or nextest status output.
- An opt-in nextest profiling recipe records the run without changing
  nextest parallelism.
- Profiled runs leave a replayable nextest archive and a
  Chrome/Perfetto trace under a stable `tmp/*-latest` path.
- A first profiling run identifies long-pole tests, serialized groups,
  idle slots, and retry-heavy tests without manual XML/log digging.
- Fixed sleeps that are not semantically required are replaced by
  readiness waits with explicit deadlines.
- Remaining test-body builds are either moved to fixture staging or
  documented as intentional.
- Any raised nextest concurrency cap is backed by port/domain/build-dir
  isolation notes and a before/after timing comparison.
