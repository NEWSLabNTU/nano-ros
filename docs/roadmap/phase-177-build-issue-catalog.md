# Phase 177 - Build/Test Issue Tracker

**Goal.** Track known build and test issues found during the 2026-05-20/21
post-refactor sweeps of `main`. Use this file as an issue tracker:
open items stay in "Known issues"; completed items move to "Closed".

**Scope.** `just setup`, `just ci`, `just build-all`, and the `test-all`
tail. Issues owned by a more specific phase are linked here but should be
resolved in that owning phase.

**Current status.** Phase 171 is archived and this tracker now owns the
remaining build/test cleanup. Build quality gates are green after the
follow-up fixes, but the full runtime `test-all` layer still has
environment/setup and E2E failures that need focused owners. Latest
171.F.1 root `just ci` attempt (2026-05-22, with
`NROS_ZEPHYR_BUILD_ROOT=/home/aeon/repos/nano-ros/build/zephyr-workspace-builds`)
passed static checks, RTOS link check, Cyclone CI, doctests, Miri, C
codegen, and orchestration E2E, then failed in `test-all` with 39 real
failures plus 8 environment skips.

## Setup Contract

Run the full sweep in this order:

- [x] `just setup`
- [x] `just build-test-fixtures`
- [ ] `just test-all`

`test-all` should consume fixture binaries built by
`just build-test-fixtures`; it should not spend its runtime compiling
examples. Rust fixture lookup must use the `nros-fast-release` Cargo
profile directory, C/C++ fixture lookup must use the matching CMake
`build-<rmw>` directory, and missing host tools should skip with an
actionable setup remedy instead of surfacing as product failures.

The 2026-05-22 rerun followed this setup sequence. `just setup` passed,
`just build-test-fixtures` passed, and the follow-up `just test-all`
completed with 960 tests run: 911 passed, 49 failed, and 9 skipped.
Doctests, Miri, C codegen, C message generation, and orchestration E2E
passed.

## Known Issues

### Build/Feature Ownership

- [x] **177.3 - Cyclone CMake/Corrosion path for Rust examples.**
  Closed 2026-05-25 by the merged Phase 175 work.
  `nros_rmw_cyclonedds_register` lives only in the C++/CMake build, so
  `cargo build --features rmw-cyclonedds` of native/freertos/threadx
  Rust examples cannot link it directly; Cyclone-backed fixtures must go
  through the CMake/Corrosion path. Phase 175 landed that path for native
  Rust and added embedded Cyclone fixture wiring for FreeRTOS and ThreadX.
  FreeRTOS Rust Cyclone boots and exchanges user data. ThreadX RISC-V64
  now builds the Cyclone `ddsc` static-library probe and links the C,
  C++, and Rust talker/listener fixtures. The original build/link
  ownership issue is closed; remaining ThreadX runtime diagnosis is
  tracked separately under 177.22.

- [ ] **177.22 - ThreadX Cyclone participant init runtime trap.**
  Owner: Phase 177 runtime/Cyclone follow-up.
  ThreadX RISC-V64 Cyclone fixtures now build and link, but runtime
  still fails during Cyclone participant initialization before RTPS
  traffic starts.
  The 2026-05-24 manual two-QEMU probe boots ThreadX, initializes NetX Duo
  and BSD sockets, then reports `nros_support_init -> -1` on the listener;
  the talker traps with `mcause=0x7` at picolibc tinystdio
  `__file_str_put` (`mepc=0x80074270`, `mtval=0x10016c008`,
  `tinystdio/filestrput.c:44`). Phase 175 fixed the prerequisite
  allocation/link issues (`z_malloc`/`z_free`, C++ `new/delete`,
  Cyclone session-state allocation, and `stderr` binding). The remaining
  bug is to diagnose the Cyclone/picolibc stdio string-buffer state used
  during participant initialization on ThreadX.

### Test-All Environment / Setup

- [x] **177.6 - PX4 tests require explicit PX4 workspace setup.**
  `test-all` failures include missing or invalid `PX4_AUTOPILOT_DIR`.
  Fixed: `just/sdk-env.just` now provides the repo-local default
  `PX4_AUTOPILOT_DIR`, `.env.example` documents position-independent
  overrides, and PX4 tests consume only that environment variable with
  the exact setup remedy when it is invalid.

- [x] **177.7 - ESP-IDF and PlatformIO host tools missing.**
  ESP-IDF and PlatformIO groups require `idf.py` and `pio`; the minimal
  sweep environment did not provide them. Fixed: the ESP-IDF smoke
  detects the env shim path supplied by `NROS_ESP_IDF_ENV_SHIM`, and
  `just/sdk-env.just` defines the default ESP-IDF workspace, env shim, and
  user-local tool PATH used by PlatformIO. `.env.example` documents
  overrides, while `.envrc` remains optional direnv glue for loading
  `.env`. Full `just setup` already includes `platformio`, `esp_idf`,
  and `px4` in the `everything` tier.

- [ ] **177.8 - Full runtime matrix requires prebuilt fixtures.**
  The latest sweep was run after `just setup` and
  `just build-test-fixtures`, so the remaining fixture/setup failures are
  narrower than the original broad prebuild issue. Keep this item open
  until every fixture lookup uses the build-fixture artifact layout and
  every optional host dependency reports a precise skip/remedy.

- [ ] **177.22 - Zephyr CycloneDDS fixtures fail after Cyclone setup.**
  Rechecked 2026-05-25 after `just setup all`. `just cyclonedds doctor`
  passes and the expected host artifacts exist at
  `build/install/bin/idlc` and `build/install/lib/libddsc.so`, so this is
  no longer a missing CycloneDDS setup/install issue. The Zephyr fixture
  prebuild now attempts the CycloneDDS cells, but `just zephyr
  build-fixtures` fails for all Rust, C, and C++ CycloneDDS fixture
  variants. The common compile blocker is
  `packages/dds/nros-rmw-cyclonedds/src/internal.hpp::platform_now_ms()`:
  the fallback path calls
  `std::chrono::steady_clock::now().time_since_epoch()` and
  `std::chrono::duration_cast`, but Zephyr's minimal C++ chrono shim used
  by `native_sim` does not expose those APIs. Route Zephyr through the
  existing platform clock shim instead; `zephyr/nros_platform_zephyr_shims.c`
  already provides `nros_platform_clock_ms()` via `k_uptime_get()`. Do not
  rerun full Zephyr E2E until `just zephyr build-fixtures` is green again.

### Test-All Runtime / E2E

- [ ] **177.9 - Runtime E2E failures need focused reruns.**
  The 2026-05-22 `test-all` rerun reported 960 tests run: 911 passed, 49
  failed, and 9 skipped after `just setup` and `just build-test-fixtures`
  both passed. The remaining failures are grouped below so owners can
  close them independently. Newer focused fixes closed 177.19 and 177.20;
  rerun these groups with required fixtures/services prebuilt and split
  remaining product bugs from host/setup fallout.

#### 2026-05-22 Failed Tests by Group

- [x] **177.9.A - Host tools, fixture gates, and explicit prerequisites.**
  Focused rerun on 2026-05-25:
  `cargo nextest run --cargo-profile nros-fast-release -p nros-tests
  --no-fail-fast --test bridge_xrce_to_dds_e2e --test
  bridge_zenoh_to_dds_e2e --test integration_esp_idf --test
  integration_px4 --test cpp_parameters`.
  Result: 3 passed, 2 environment-skipped, 0 real failures after applying
  the project `[SKIPPED]` classifier. The SDK-dependent tests were also
  rerun through `just _nextest-platform <test-binary>` so
  `just/sdk-env.just` provided the repo-local SDK defaults, and direct
  Cargo was verified with `source scripts/sdk-env.sh` before invoking
  `cargo nextest`.
  - [x] `bridge_xrce_to_dds_e2e::bridge_xrce_to_dds_starts_and_opens_both_sessions`
        now reports the missing retired source path explicitly; the old
        `examples/native/c/bridge/xrce-to-dds` tree is not present in the
        current collapsed examples layout.
  - [x] `bridge_zenoh_to_dds_e2e::bridge_zenoh_to_dds_starts_and_opens_both_sessions`
        now reports the missing retired source path explicitly; the old
        `examples/bridges/native-rust-zenoh-to-dds` tree is not present in
        the current collapsed examples layout.
  - [x] `integration_esp_idf::esp_idf_integration_shell_smoke` passes when
        run via `just`, which exports `NROS_ESP_IDF_ENV_SHIM` and
        `IDF_PATH` from `just/sdk-env.just`.
  - [x] `integration_px4::px4_integration_template_smoke` passes when run
        via `just`, which exports `PX4_AUTOPILOT_DIR` from
        `just/sdk-env.just`.
  - [x] `cpp_parameters::cpp_parameters_roundtrip` passes.

- [x] **177.9.B - Platform CMake, logging, and NuttX smoke coverage.**
  These are build/smoke edges inside the test layer, not the main
  `build-test-fixtures` prebuild path:
  The five environment skips from the focused 2026-05-25 rerun are not
  generic `just setup` misses. Four are intentionally deferred raw-CMake
  smoke cells whose real coverage lives in platform-aware recipes; the
  NuttX skip means `just nuttx build-fixtures-make` was not rerun after
  the local NuttX kernel was configured/built without nano-ros external
  apps.
  - [x] `cmake_platform_matrix::cmake_platform_freertos` is an intentional
        environment skip; the raw CMake smoke does not supply
        `FREERTOS_DIR` + `LWIP_DIR`, so FreeRTOS coverage stays in the
        platform recipes.
  - [x] `cmake_platform_matrix::cmake_platform_nuttx` is an intentional
        environment skip; NuttX builds through cargo / `just nuttx build`,
        not the raw CMake smoke.
  - [x] `cmake_platform_matrix::cmake_platform_threadx` is an intentional
        environment skip; ThreadX coverage is owned by the ThreadX Linux
        integration shell and board-aware recipes.
  - [x] `cmake_platform_matrix::cmake_platform_zephyr` is an intentional
        environment skip; Zephyr coverage is owned by west/module builds.
  - [x] `logging_smoke::logging_smoke_freertos_mps2_emits_every_severity`
        passes.
  - [x] `logging_smoke::logging_smoke_mps2_baremetal_emits_every_severity`
        passes.
  - [x] `logging_smoke::logging_smoke_nuttx_qemu_arm_emits_every_severity`
        passes.
  - [x] `logging_smoke::logging_smoke_threadx_linux_harness_captures_nros_log_stderr`
        passes after refreshing the ThreadX log writer in app-thread context
        and emitting each Linux stderr record with one host syscall.
  - [x] `logging_smoke::logging_smoke_threadx_riscv64_emits_every_severity`
        passes.
  - [x] `logging_smoke::logging_smoke_zephyr_native_sim_emits_every_severity`
        passes.
  - [x] `nuttx_make_e2e::nuttx_external_apps_link_into_kernel_binary`
        now classifies a configured kernel with zero nano-ros external-app
        symbols as a stale make fixture environment skip; partial symbol loss
        still fails.
  - [x] Focused verification:
        `cargo nextest run --cargo-profile nros-fast-release -p nros-tests
        --no-fail-fast --test cmake_platform_matrix --test logging_smoke
        --test nuttx_make_e2e` produced 9 passes, 5 environment skips, and
        `just _count-real-failures target/nextest/default/junit.xml` returned
        `0`.

- [ ] **177.9.C - Native C/XRCE runtime.**
  - [ ] `c_xrce_api::test_c_xrce_listener_starts`
  - [ ] `c_xrce_api::test_c_xrce_talker_listener_communication`
  - [ ] `c_xrce_api::test_c_xrce_talker_starts`

- [ ] **177.9.D - QEMU RTIC and QEMU zenoh/serial runtime.**
  - [ ] `emulator::test_qemu_rtic_action_e2e`
  - [ ] `emulator::test_qemu_rtic_mixed_priority_pubsub_e2e`
  - [ ] `emulator::test_qemu_rtic_pubsub_e2e`
  - [ ] `emulator::test_qemu_rtic_service_e2e`
  - [ ] `emulator::test_qemu_serial_pubsub_e2e`
  - [ ] `large_msg::test_qemu_zenoh_large_publish`

- [ ] **177.9.E - XRCE runtime.**
  - [ ] `xrce::test_xrce_action_fibonacci`
  - [ ] `xrce::test_xrce_multiple_messages`
  - [ ] `xrce::test_xrce_service_request_response`
  - [ ] `xrce::test_xrce_talker_listener_communication`

- [ ] **177.9.F - Zephyr native/cross E2E runtime.**
  - [ ] `test_bidirectional_native_zephyr_e2e`
  - [ ] `test_native_server_zephyr_client`
  - [ ] `test_native_talker_to_zephyr_cpp_listener`
  - [ ] `test_native_to_zephyr_e2e`
  - [ ] `test_zephyr_action_e2e`
  - [ ] `test_zephyr_cpp_action_server_to_client_e2e`
  - [ ] `test_zephyr_cpp_service_server_to_client_e2e`
  - [ ] `test_zephyr_cpp_talker_to_listener_e2e`
  - [ ] `test_zephyr_cpp_talker_to_native_listener`
  - [ ] `test_zephyr_to_native_e2e`
  - [ ] `test_zephyr_talker_to_listener_e2e`
  - [ ] `test_zephyr_xrce_c_talker_listener`
  - [ ] `test_zephyr_xrce_cpp_action_e2e`
  - [ ] `test_zephyr_xrce_cpp_service_e2e`
  - [ ] `test_zephyr_xrce_cpp_talker_listener`
  - [ ] `test_zephyr_xrce_rust_action_e2e`
  - [ ] `test_zephyr_xrce_rust_service_e2e`
  - [ ] `test_zephyr_xrce_rust_talker_listener`

- [ ] **177.9.G - NuttX action E2E runtime.**
  - [ ] `rtos_e2e::test_rtos_action_e2e::platform_2_Platform__Nuttx::lang_2_Lang__C`
  - [ ] `rtos_e2e::test_rtos_action_e2e::platform_2_Platform__Nuttx::lang_3_Lang__Cpp`

- [ ] **177.9.H - Flaky but recovered.**
  This passed on retry and should be watched separately from hard
  failures:
  - [ ] `rtos_e2e::test_rtos_pubsub_e2e::platform_3_Platform__ThreadxLinux::lang_3_Lang__Cpp`

- [x] **177.19 - ESP32-C3 QEMU OpenETH Zenoh pub/sub does not move user data.**
  Fixed the ESP32-C3 QEMU Zenoh examples by sizing their generated
  executor arena for pub/sub instead of carrying the default action-capable
  74 KB arena on the main stack. The oversized stack-local `Executor`
  overflowed into adjacent `.bss`, clearing the smoltcp poll-callback
  slot after Ethernet init had registered it; runtime diagnostics showed
  `cb_registered=false` and `cb_sets=0` while `do_poll` climbed. The
  examples now set `NROS_EXECUTOR_ARENA_SIZE=16384` and trim Zenoh's
  unused UDP socket slots with `NROS_SMOLTCP_MAX_UDP_SOCKETS=2`. Focused
  verification passed:
  `XDG_RUNTIME_DIR=/tmp TMPDIR=/tmp cargo test -p nros-tests --test esp32_emulator test_esp32_talker_listener_e2e -- --nocapture`
  (`1 passed`, `8.66s`).

## Closed

- [x] **177.22 - Make `nros` the canonical build/test CLI.**
  Closed 2026-05-25. Build and test recipes should not compile the
  `nros-cli` binary as a side effect, and should not use or provide the
  legacy `cargo nano-ros` command. Setup owns installing the canonical
  `nros` binary (`just setup base` via workspace cargo tools).
  Later stages resolve `nros` from `PATH` or `NROS_CLI=/path/to/nros`
  and fail with an actionable setup hint if it is missing. Root
  binding generation, native fixture generation, FreeRTOS examples, and
  the Zephyr Rust generated-dir preflight now use that canonical
  resolver. The old `cargo-nano-ros` package remains only as an internal
  Rust library until its codegen APIs are renamed or split; it no longer
  builds a Cargo subcommand binary.

- [x] **177.21 - `generate-bindings` should be incremental.**
  Closed 2026-05-24. Fixed with
  `scripts/build/generate-rust-incremental.sh`. Root `generate-bindings`
  now hashes the package manifest, local interface files, the built
  `nros` binary, ROS interface prefixes, and generator args before
  deciding whether to call `nros generate-rust --force`. Unchanged
  packages skip regeneration; package/interface/generator changes still
  force a refresh.

- [x] **177.2 - Remaining Cyclone Zephyr action gaps.**
  Closed 2026-05-23. Zephyr Cyclone DDS action examples now build and
  run end-to-end for C, C++, and Rust on `native_sim`. The fix adds a
  shared Zephyr CMake helper that generates and links the Cyclone DDS
  descriptors required by action endpoints:
  `builtin_interfaces/Time`, `unique_identifier_msgs/UUID`,
  `action_msgs/{GoalInfo,GoalStatus,GoalStatusArray,CancelGoal}`, and
  `example_interfaces/action/Fibonacci`. The action overlays also use
  NSOS host sockets and larger heap/pthread resources, avoiding the old
  zeth/TAP panic path. The test harness now treats Zephyr fixtures as
  prebuilt inputs and reports stale/missing binaries with the `just
  zephyr build-fixtures` remedy instead of building inside tests.
  Focused verification passed:
  `NROS_ZEPHYR_FIXTURE_FILTER='build-(rs|c|cpp)-action-(server|client)-cyclonedds' NROS_ZEPHYR_BUILD_JOBS=1 NROS_ZEPHYR_NINJA_JOBS=8 just zephyr build-fixtures`,
  `XDG_RUNTIME_DIR=/tmp TMPDIR=/tmp cargo test -p nros-tests --test zephyr test_zephyr_dds_cpp_action_e2e -- --nocapture --test-threads=1`,
  `XDG_RUNTIME_DIR=/tmp TMPDIR=/tmp cargo test -p nros-tests --test zephyr test_zephyr_dds_c_action_e2e -- --nocapture --test-threads=1`,
  and
  `XDG_RUNTIME_DIR=/tmp TMPDIR=/tmp cargo test -p nros-tests --test zephyr test_zephyr_dds_rs_action_e2e -- --nocapture --test-threads=1`.
  The aemv8r/FVP reference path remains a separate platform
  re-verification item if that target is re-enabled.

- [x] **177.20 - QEMU MPS2 serial Zenoh pub/sub stalls inside publish path.**
  Fixed in the `zenoh-pico` submodule by starting publisher write filters
  open for `Z_FEATURE_MULTI_THREAD == 0` builds. Single-threaded embedded
  clients do not have a background read task to learn remote subscriber
  matches before the application's first write, so the previous default
  suppressed the first serial publish before it reached the router.
  Verified 2026-05-23 with
  `XDG_RUNTIME_DIR=/tmp TMPDIR=/tmp cargo test -p nros-tests --test emulator test_qemu_serial_pubsub_e2e -- --nocapture`
  (`published=1, received=1`, 7.68 s).

### Closed in the original 2026-05-20/21 sweep

- [x] **177.1 - CycloneDDS Zephyr duplicate `NSOS_MID_IPPROTO_IP` case.**
  `native-sim-ipproto-ip-patch.sh` already added a complete IPPROTO_IP
  case to `nsos_adapt_setsockopt`; the redundant 11W.12 patch added a
  second label and caused `duplicate case value`. Fixed by making 11W.12
  skip when the case is already present. This was the original sole
  `build-all` blocker.

- [x] **177.4 - ESP-IDF setup git-ref corruption.**
  `scripts/esp_idf/setup.sh` used `fetch origin v5.3:v5.3`, which tried
  to write the annotated `v5.3` tag into `refs/heads/v5.3`. Fixed in
  `6be211ee4` with `fetch --depth 1 --tags origin <ref>` plus
  `checkout <ref>`.

- [x] **177.5 - NuttX/ESP32 `-Z build-std` e2e failures.**
  Verified green with pinned `nightly-2026-04-11` plus `rust-src`.
  Added `build_std_nightly_skip()` so missing toolchains skip with the
  exact remedy instead of failing with an opaque missing `core` error.

- [x] qemu `build-zenoh-pico.sh` missing
  `nros-platform-cffi/include` and `c/zpico` include paths.

- [x] `justfile build-workspace` needed to exclude no_std/C/C++ staticlib
  packages from the `nextest --no-run` line.

- [x] `nros/src/lib.rs` needed `sched_context` re-export gated on
  `rmw-cffi`.

- [x] `nros-c` / `nros-cpp` `build.rs` needed the picolibc `-isystem`
  include for riscv64-none `cc::Build`.

- [x] Stale pre-collapse `rust/{zenoh,dds}/<ex>` fixture paths were
  removed from native/freertos/threadx/nuttx recipes.

- [x] dust-dds Rust examples migrated to `nros-rmw-cyclonedds-sys`; bare
  metal fixture matrices reverted to zenoh-only.

- [x] Unified jobserver `gmake` to make-4.4 alias fixed the stray make
  4.3 fifo jobserver failure.

### Closed in the 2026-05-21 follow-up sweep

- [x] **177.10 - Invalid `just ci/build-all` command path.**
  `just ci/build-all` is not a recipe. The correct split is `just ci`
  for quality/test orchestration and `just build-all` for the build
  matrix.

- [x] **177.11 - Clippy doc-comment lazy continuation.**
  Fixed in `nros-rmw-cyclonedds-sys`.

- [x] **177.12 - Stale example build directories confused checks.**
  Removed generated `examples/**/build*` directories so example checks no
  longer recurse into nested Corrosion workspaces.

- [x] **177.13 - `nros-c` library tests missing platform log symbols.**
  Added weak fallback stubs for `nros_platform_log_write` and
  `nros_platform_log_flush`.

- [x] **177.14 - NuttX C/C++ opaque size asserts.**
  Size probing returned no usable constants for the custom target. The
  C/C++ build scripts now use committed NuttX fallback sizes when the
  probe returns empty or zero sizes.

- [x] **177.15 - Zephyr read-only workspace/cache failures.**
  The Zephyr recipe now uses repo-local writable build/cache roots when
  the sibling Zephyr workspace or toolchain cache path is read-only.

- [x] **177.16 - Zephyr native_sim read-only ccache temp path.**
  Zephyr's built-in `ccache` wrapper wrote under read-only
  `/run/user/.../ccache-tmp`. The recipe disables that path with
  `USE_CCACHE=0` while preserving the repo-controlled `sccache` compiler
  launcher.

- [x] **177.17 - Zephyr CycloneDDS compatibility gaps.**
  Added/fixed `steady_clock::time_point`, `THREAD_CUSTOM_DATA`, weak
  `nsos_adapt_getifaddrs`, and non-fatal Cortex-R Rust patch handling
  when upstream Kconfig is not writable.

- [x] **177.18 - Zephyr native_sim inherited fifo jobserver failure.**
  `just build-all` can run Zephyr under the unified make-4.4 fifo
  jobserver, but Zephyr native_sim's final runner link invokes
  CMake's `MAKE` cache entry from `scripts/native_simulator/Makefile`.
  Ubuntu make 4.3 aborts on `--jobserver-auth=fifo:...` with
  `invalid --jobserver-auth string`. Zephyr build recipes now prepend the
  repo-local `third-party/make` and pass `-DMAKE=<repo>/third-party/make/make`
  so the native_sim make hop uses GNU make 4.4 and remains on the shared
  jobserver.

## Verification Notes

- [x] `cargo +nightly-2026-04-11 fmt --check`
- [x] `XDG_RUNTIME_DIR=/tmp TMPDIR=/tmp just check`
- [x] `cargo test --no-run -p nros-c --lib`
- [x] `just nuttx build-fixtures`
- [x] One clean Zephyr `native_sim` fixture with the fixed flags.
- [x] Zephyr native_sim runner make-hop with poisoned fifo `MAKEFLAGS`
  routed through repo-local GNU make 4.4 instead of `/usr/bin/make`.
- [x] 2026-05-22 `just setup`.
- [x] 2026-05-22 `just build-test-fixtures`.
- [x] 2026-05-22 `just test-all` completed after setup and fixture
  prebuild: 911 passed, 49 failed, and 9 skipped. Remaining failures are
  grouped under 177.9.
- [ ] Full `just build-all` rerun after the final Zephyr follow-up fix.
- [~] Full root `just ci` rerun after Phase 171 archive prep: static
  gates passed, `test-all` failed with 39 real failures + 8 environment
  skips.
- [ ] Full `test-all` rerun with PX4/ESP-IDF/PlatformIO/bridge fixtures
  prepared and 177.19/177.20 either fixed or explicitly expected-failed.
- [ ] Full green `test-all` rerun after 177.9 fixture/setup/runtime
  groups close.

## Archive Rule

Archive this tracker only after:

- [x] 177.3 closes or moves into a newer, more specific phase doc.
- [ ] 177.6 through 177.9 have owners and either close or move into more
  specific phase docs.
- [x] 177.19 and 177.20 close or move into platform-specific runtime
  phases.
