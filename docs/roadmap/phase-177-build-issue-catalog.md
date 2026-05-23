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

## Known Issues

### Build/Feature Ownership

- [ ] **177.3 - Cyclone for pure-cargo Rust examples.**
  Owner: Phase 175.
  `nros_rmw_cyclonedds_register` lives only in the C++/CMake build, so
  `cargo build --features rmw-cyclonedds` of native/freertos/threadx
  Rust examples cannot link it. 175.A landed the native
  `examples/native/rust/{talker,listener}/CMakeLists.txt` CMake path and
  fixed native two-process user data. 175.B remains: embedded ddsrt port
  and bare-metal Cyclone enablement.

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
  Several runtime/bridge groups failed because required binaries or
  fixtures were not prebuilt before `test-all`. Clarify the required
  `just build-test-fixtures` / service setup sequence or make `test-all`
  detect and report missing fixtures precisely.

### Test-All Runtime / E2E

- [ ] **177.9 - Runtime E2E failures need focused reruns.**
  The latest root `just ci` tail reports 39 real failures plus 8
  environment skips. Failures cluster in bridge startup/session-open,
  CMake platform cells, ESP32/QEMU serial, C XRCE API, logging smoke,
  NuttX/RTOS E2E, native XRCE actions/services/pubsub, and Zephyr E2E
  groups. Rerun these groups with required fixtures/services prebuilt
  and split real product bugs from host/setup fallout.

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
- [ ] Full `just build-all` rerun after the final Zephyr follow-up fix.
- [~] Full root `just ci` rerun after Phase 171 archive prep: static
  gates passed, `test-all` failed with 39 real failures + 8 environment
  skips.
- [ ] Full `test-all` rerun with PX4/ESP-IDF/PlatformIO/bridge fixtures
  prepared and 177.19/177.20 either fixed or explicitly expected-failed.

## Archive Rule

Archive this tracker only after:

- [ ] 177.3 closes or moves into a newer, more specific phase doc.
- [ ] 177.6 through 177.9 have owners and either close or move into more
  specific phase docs.
- [x] 177.19 and 177.20 close or move into platform-specific runtime
  phases.
