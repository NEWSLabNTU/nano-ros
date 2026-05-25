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

#### 179.G sleep audit

Initial 179.G pass replaced the named fixed sleeps with bounded
readiness/count waits:

- `c_xrce_api.rs`: startup waits now watch for `Support initialized`;
  talker/listener communication waits for three `Received` lines instead
  of sleeping for the whole message window.
- `custom_transport_loopback.rs`: fixed sleeps removed (2026-05-25). The
  test now waits for the listener's `Subscriber created` readiness marker
  and for bounded `Published:` / `Received:` counts. This was unblocked by
  fixing the underlying custom-link runtime deadlock — see the resolved
  follow-up below.
- `zero_copy.rs`: subscription-propagation sleeps were removed; tests
  wait for three received messages or two `seq=` trace markers.
- `safety_e2e.rs`: subscription-propagation sleeps were removed; tests
  wait for three `crc=ok` messages or two standard `Received:` messages.
- `ros2_lifecycle_interop.rs`: rmw_zenoh graph and post-configure waits
  now poll the ROS 2 lifecycle CLI until `/lifecycle_demo` or `inactive`
  appears.

Follow-up 179.G slices also replaced fixed sleeps in `large_msg.rs`,
`qos.rs`, `phase_118_collapse.rs`, `multi_node.rs`,
`error_handling.rs`, `rmw_interop.rs`, and `zephyr.rs`. Remaining sleeps
in these audited files are limited to bounded poll/backoff intervals, a
throughput benchmark's intentional measurement window, the documented
custom-transport readiness gap, and the documented Zephyr XRCE action
propagation guard.

#### 179.G remaining follow-ups

The post-audit rerun found four follow-ups:

- `custom_transport_loopback.rs` **resolved 2026-05-25.** The hang was a
  full-duplex deadlock in the example transport bridge, not a flaky
  readiness race. The talker/listener bridged the socket through a single
  `Mutex<Option<TcpStream>>`; zenoh-pico drives reads on a dedicated
  read-task thread while the main/tx thread writes, so the read task held
  the mutex across its back-to-back blocking `recv` and starved the tx
  thread. `create_publisher` → `z_declare_publisher` → `_z_add_interest`
  → the write callback blocked forever on `bridge.stream.lock()` (gdb
  all-thread backtrace). Fix: drop the mutex and use `&TcpStream`, which
  implements `Read`/`Write` for shared refs, so the two directions run
  concurrently — exactly how zenoh-pico's native `tcp.c` drives the raw
  fd from both threads. With the runtime path fixed, the fixed sleeps
  were replaced by readiness/count waits (test now passes in ~0.5s with
  `Published: 20, Received: 19`). The two example binaries are now built
  by `just native build-fixture-extras` (with `generate-rust` codegen) so
  `just build-test-fixtures` stages them for `just test-all`.
- `threadx_riscv64 build-fixtures` failed in the CycloneDDS C / C++
  fixture link with unresolved `dds_*` / `ddsrt_*` / `ddsi_*` symbols
  from `libnros_rmw_cyclonedds.a`. **Root-caused and fixed 2026-05-25.**
  The cross `libddsc.a` *was* on the link line (whole-archived with the
  backend), and GNU `nm` saw all 498 `dds_*` symbols — but `llvm-nm`
  saw zero, only a `__gnu_lto_slim` marker. The archive held GCC
  *slim-LTO* objects (GIMPLE bytecode, no machine code); rust-lld, the
  linker for the ThreadX examples, cannot consume GCC LTO objects
  without GCC's LTO plugin, so every Cyclone symbol was undefined even
  under `--whole-archive`. The cross-probe script already sets
  `-fno-lto` + `ENABLE_LTO=OFF` (added 2026-05-24), but the build dir
  carried a stale `ENABLE_LTO:BOOL=ON` cache from a 2026-05-23 configure
  and an incremental `--mode build` reused it. Fix:
  `scripts/cyclonedds/threadx-cross-probe.sh` now wipes the build dir for
  a clean reconfigure whenever the cached LTO setting is not
  `ENABLE_LTO:BOOL=OFF`, so the rebuild emits real linkable objects.
  After a clean rebuild, `llvm-nm` resolves the symbols and the C and
  C++ ThreadX-RV64 Cyclone talkers link to RISC-V ELF executables. The
  fixtures stay gated behind `NROS_THREADX_RV64_CYCLONEDDS_FIXTURES=1`
  (run `just cyclonedds threadx-cross-probe` first); `just setup all`
  still does not cross-build Cyclone, so the gate keeps the default
  `threadx_riscv64 build-fixtures` path provisioning-free.
- Zephyr tests could report stale or missing fixtures after
  `just zephyr build-fixtures` because the build recipe falls back to
  `build/zephyr-workspace-builds` when the sibling workspace is not
  writable, while the test resolver still defaulted to
  `zephyr-workspace`. The resolver now mirrors the build recipe and
  still honors `NROS_ZEPHYR_BUILD_ROOT`.
- Zephyr fixture stale checks watched all RMW source trees for every RMW,
  so a CycloneDDS edit could make Zenoh or XRCE fixtures look stale. The
  check now watches only the backend package that matches the fixture
  RMW, with an all-backend fallback for unknown names.

Zephyr CycloneDDS `native_sim` runtime failures remain open after the
fixture-resolution cleanup. **Root cause identified 2026-05-25** (this is
a port defect, not a fixed-sleep regression):

`zephyr/CMakeLists.txt` globs ddsrt's POSIX backends — `file(GLOB
_cdds_ddsrt_posix ${CYCLONEDDS_DIR}/src/ddsrt/src/*/posix/*.c)` — which
pulls in `src/threads/posix/threads.c`, the raw `pthread_create` thread
port. On `native_sim` the Zephyr kernel runs atop the native simulator,
which models every `k_thread` as a host thread under its own control.
CycloneDDS spawns ~7 worker threads (recv / dq.builtins / tev / gc / …)
via raw `pthread_create` at `dds_create_participant`; those host threads
are created outside the simulator's bookkeeping, so each one trips the
kernel's `os: tid 0x... is in use!` check the moment it calls a Zephyr
API. Reproduced directly: `build/zephyr-workspace-builds/build-cpp-
talker-cyclonedds/zephyr/zephyr.exe` logs seven `tid ... is in use!`
errors at participant creation; a talker+listener pair exchanges **zero**
messages (data plane dead). The talker's own publish loop still runs
because it is the main `k_thread`, which is why the boot-banner tests
pass while the e2e tests time out.

The correct fix is a `k_thread`-based ddsrt Zephyr thread port (new
`threads/zephyr` backend using `k_thread_create` + dynamic stacks,
excluded from the POSIX glob), parallel to the existing `freertos` and
`threadx` ports. That is a bounded but non-trivial RTOS port and belongs
to the Zephyr CycloneDDS work tracked under Phase 177.2; it is not a
test-harness fix. Until then these `native_sim` Cyclone e2e tests stay in
the serial `zephyr-native-cyclonedds` group and are expected to fail at
runtime.

### Post-nextest stages have poor visibility

Doctests, Miri, C codegen, and orchestration E2E run after nextest and
outside nextest scheduling. The top-level summary is printed before
those later stages, so a slow or failing late stage is not reflected in
the same timing/reporting surface as the workspace tests.
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

- [x] **179.B - add opt-in nextest profiling options.** Do not add new
  public recipes. Keep `just test` and `just test-all` as the only
  user-facing entry points, and enable profiling with environment
  options:

  ```sh
  NROS_NEXTEST_RECORD=1 just test
  NROS_NEXTEST_RECORD=1 just test-all
  ```

  Implement this through a shared private nextest runner helper so the
  normal and profiled paths use the same nextest arguments, filters,
  cargo profile args, verbose handling, and parallelism. Profiling must
  preserve the existing nextest execution model; it only enables
  recording and artifact export.

  Suggested knobs:

  - `NROS_NEXTEST_RECORD=1`: enable nextest run recording and artifact
    export.
  - `NROS_NEXTEST_RECORD_DIR=<path>`: override the timestamped output
    directory.
  - `NROS_NEXTEST_REPLAY_LOG=1`: emit a full replay log with captured
    stdout/stderr. Keep this optional because successful-test output can
    be large.
  - `NROS_NEXTEST_TRACE_GROUP_BY=slot|binary`: choose Perfetto grouping;
    default to `slot` for wall-clock/concurrency analysis.
  - `NROS_NEXTEST_RECORD_KEEP_STATE=1`: keep the temporary
    `NEXTEST_STATE_DIR`; otherwise remove it after archive/trace export.

  Default artifact layout:

  ```text
  tmp/nextest-profile-test-YYYYMMDD-HHMMSS/
  tmp/nextest-profile-test-latest -> nextest-profile-test-YYYYMMDD-HHMMSS
  tmp/nextest-profile-test-all-YYYYMMDD-HHMMSS/
  tmp/nextest-profile-test-all-latest -> nextest-profile-test-all-YYYYMMDD-HHMMSS
  ```

  Landed as `scripts/test/nextest-profile.sh`, sourced by `just test`
  and `just test-all`, plus the persistent
  `.config/nextest-record.toml` overlay for nextest's experimental
  recording mode. The default path is unchanged; setting
  `NROS_NEXTEST_RECORD=1` enables recording and artifact export while
  preserving the existing nextest args, filters, cargo profile, verbose
  handling, and parallelism.

- [x] **179.C - export replayable nextest logs.** For profiled runs,
  export the latest recording with `cargo nextest store export latest`.
  Write `nextest-run.zip` under the profile directory so a failing run
  can be replayed with full captured output, including successful test
  output when needed. Use a profile-local `NEXTEST_STATE_DIR` so opt-in
  profiling does not pollute the user's global nextest record store.
  Landed with `nextest-run.zip` export and optional
  `NROS_NEXTEST_REPLAY_LOG=1` replay-log generation.

- [x] **179.D - export Perfetto timeline traces.** For profiled runs,
  export `cargo nextest store export-chrome-trace latest --group-by slot`
  to `nextest-trace.json`, or use the `NROS_NEXTEST_TRACE_GROUP_BY`
  override. This is the canonical artifact for concurrency, group
  bottlenecks, idle slots, retries, and long-pole visualization. Also
  copy `target/nextest/default/junit.xml` to the profile directory as
  `junit.xml`.
  Landed with `nextest-trace.json` and `junit.xml` in each profile
  directory. `NROS_NEXTEST_TRACE_GROUP_BY` accepts `slot` or `binary`
  and defaults to `slot`.

- [ ] **179.E - document profiling overhead and retention.** Recording
  adds event/output-store writes and archive export can create sizable
  artifacts on chatty tests. Keep recording opt-in for local runs,
  avoid `--no-capture` because it serializes execution, and document
  the profile env vars in the test section. If `NROS_NEXTEST_REPLAY_LOG`
  is enabled, write `nextest-replay.log`; otherwise rely on the portable
  archive for full replay.

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

- [x] **179.G - audit and remove fixed sleeps.** Replace fixed sleeps in
  E2E tests with readiness polling, log-pattern waits, port-open waits,
  or first-message deadlines. Keep upper bounds so failures still time
  out clearly. Start with C XRCE API, custom transport loopback,
  zero-copy, safety E2E, and ROS 2 lifecycle interop.

  Completed 2026-05-25. See the 179.G sleep audit above for per-file
  details and retained-sleep rationale.

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

- [x] **179.I - re-evaluate Zephyr test serialization.** Confirm which
  Zephyr tests still configure/build inside the test body. Runtime-only
  tests that consume prebuilt images and use unique ports may be able to
  leave the historical `qemu-zephyr max-threads = 1` bottleneck without
  reintroducing the old CMake corruption.

  Completed 2026-05-25:
  - Audited `zephyr`, `integration_zephyr`, `logging_smoke`, and
    `phase_118_collapse`: Zephyr runtime tests resolve prebuilt images
    through `get_prebuilt_zephyr_example` or Phase 118 fixture resolvers;
    they no longer run `west build`/CMake inside the test body.
    `integration_zephyr` still uses `west list` only as a setup probe.
  - Restored `qemu-zephyr max-threads = 6` for non-DDS fall-through tests
    because they only boot prebuilt images, check paths/staleness, or
    inspect Zephyr setup state.
  - Routed `binary(zephyr) and test(dds)` to the existing serial
    `qemu-zephyr-dds` group so Cyclone DDS native_sim fixed RTPS ports do
    not collide while non-DDS Zephyr smoke/build checks can overlap.

- [x] **179.J - isolate ROS 2 and XRCE interop enough to parallelize.**
  Survey use of ROS domain IDs, daemon behavior, DDS discovery ports,
  XRCE Agent ports, and temp dirs. Where tests can own unique domains
  and ports, split them out of the global serialized groups.

  Completed 2026-05-25. XRCE↔ROS 2 DDS interop now isolates each
  test with an ephemeral XRCE Agent UDP port plus a per-test
  `ROS_DOMAIN_ID` applied to both the nros XRCE process and the ROS 2
  `rmw_fastrtps_cpp` CLI process. `Ros2DdsProcess` grew explicit
  domain-aware spawn helpers so DDS tests can opt into this isolation
  without changing the default domain-0 helpers. The
  `xrce_ros2_interop` nextest group is raised from 1 to 3, matching
  the three runtime checks.

  The rmw_zenoh ROS 2 interop group remains serial for now: those tests
  already use unique zenoh router ports and per-process session config
  files, but several ROS 2 list/info/param CLI helpers still rely on
  daemon-sensitive behavior (`ros2 daemon stop` before the command).
  Keep `ros2-interop` at `max-threads = 1` until every helper is
  converted to a no-daemon or otherwise process-local path.

- [x] **179.K - add focused nextest lanes.** Keep full nextest coverage
  available, but add documented filterset lanes such as runtime-only,
  ROS 2 interop, Zephyr, RTOS, or native API if profiling shows
  developers repeatedly need only one slow slice.

  Completed 2026-05-25. The focused lanes follow the current
  namespace-oriented just layout rather than adding new root aliases.
  Existing backend/platform lanes remain the public entry points:
  `just xrce test`, `just xrce test-ros2`, `just xrce test-c`,
  `just zephyr test`, `just zephyr test-xrce`, and the RTOS
  namespace tests (`just freertos test`, `just nuttx test`,
  `just threadx_linux test`, `just threadx_riscv64 test`).

  Added missing native slices under `just native`: `test-ros2-params`
  for the parameter CLI interop binary and `test-native-api` for the
  full native C/C++ API lane (`native_api` plus `cpp_parameters`).
  The existing `test-c` / `test-cpp` recipes now target real nextest
  filters inside `native_api` / `cpp_parameters` instead of the stale
  removed `c_api` / `cpp_api` binary names. The default
  `just native test` excludes these slower focused slices; `just native
  test-all` fans out through the focused lanes and the C codegen shell
  tests. Fixture-consuming lanes keep the project policy that callers
  run `just build-test-fixtures` before full-matrix use.

- [x] **179.L - add a nextest fast-fail variant.** Preserve the current
  `--no-fail-fast` full report behavior, but provide an opt-in
  fail-fast configuration for local diagnosis when a slow platform is
  already known broken.

  Completed 2026-05-25. Added a persistent
  `.config/nextest.toml` `fail-fast` profile and taught `just test` /
  `just test-all` to select a nextest run profile with:

  ```sh
  NROS_NEXTEST_RUN_PROFILE=fail-fast just test
  NROS_NEXTEST_RUN_PROFILE=fail-fast just test-all
  ```

  The default run profile still passes `--no-fail-fast`, preserving the
  full-report behavior for normal local and CI runs. Non-default
  profiles rely on nextest config, so profile-specific behavior stays in
  `.config/nextest.toml` rather than proliferating `just` recipes.
  Reporting helpers use the active profile's JUnit path, e.g.
  `target/nextest/fail-fast/junit.xml` for the fail-fast profile.

## Acceptance

- The slowest nextest tests are visible in normal output from JUnit
  parsing or nextest status output.
- `NROS_NEXTEST_RECORD=1` records `just test` and `just test-all`
  nextest runs without adding new public recipe names or changing
  nextest parallelism.
- Profiled runs leave a replayable nextest archive and a
  Chrome/Perfetto trace under stable `tmp/*-latest` paths.
- A first profiling run identifies long-pole tests, serialized groups,
  idle slots, and retry-heavy tests without manual XML/log digging.
- Fixed sleeps that are not semantically required are replaced by
  readiness waits with explicit deadlines.
- Remaining test-body builds are either moved to fixture staging or
  documented as intentional.
- Any raised nextest concurrency cap is backed by port/domain/build-dir
  isolation notes and a before/after timing comparison.
