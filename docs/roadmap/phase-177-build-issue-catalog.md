# Phase 177 - Build/Test Issue Tracker

**Goal.** Track known build and test issues found during the 2026-05-20/21
post-refactor sweeps of `main`. Use this file as an issue tracker:
open items stay in "Known issues"; completed items move to "Closed".

**Scope.** `just setup`, `just ci`, `just build-all`, and the `test-all`
tail. Issues owned by a more specific phase are linked here but should be
resolved in that owning phase.

**Current status.** Build quality gates are green after the follow-up
fixes, but the full runtime `test-all` layer still has environment/setup
and E2E failures that need focused owners.

## Known Issues

### Build/Feature Ownership

- [ ] **177.2 - Remaining Cyclone Zephyr action gaps.**
  Owner: Phase 171.0.b / 171.0.c.
  C-service request delivery is no longer an open catalog item; Phase
  171.0.a fixed the RELIABLE+VOLATILE request match race. Native
  goal/result actions are also no longer open: C, Rust, and C++
  same-language Cyclone DDS action E2E are runtime-verified, the C++
  `get_result` framing bug is fixed, and the 2026-05-21 follow-ups fixed
  the nonblocking service request match race on action `send_goal`, the
  Fibonacci `GetResult_Response` dynamic-sequence bridge, the C++ feedback
  CDR framing path, and native Cyclone feedback/status dynamic-sequence
  publishing. Remaining work is Zephyr Cyclone DDS actions plus
  cross-implementation validation. The aemv8r FVP reference path still
  needs re-verification under 171.0.c.

- [ ] **177.3 - Cyclone for pure-cargo Rust examples.**
  Owner: Phase 175.
  `nros_rmw_cyclonedds_register` lives only in the C++/CMake build, so
  `cargo build --features rmw-cyclonedds` of native/freertos/threadx
  Rust examples cannot link it. 175.A landed the native
  `examples/native/rust/{talker,listener}/CMakeLists.txt` CMake path and
  fixed native two-process user data. 175.B remains: embedded ddsrt port
  and bare-metal Cyclone enablement.

### Test-All Environment / Setup

- [ ] **177.6 - PX4 tests require explicit PX4 workspace setup.**
  `test-all` failures include missing or invalid `PX4_AUTOPILOT_DIR`.
  Decide whether the test harness should point at the checked-out PX4
  submodule/workspace automatically or skip with a clearer setup remedy.

- [ ] **177.7 - ESP-IDF and PlatformIO host tools missing.**
  ESP-IDF and PlatformIO groups require `idf.py` and `pio`; the minimal
  sweep environment did not provide them. Add preflight skips/remedies or
  move these groups behind an explicit host-capability gate.

- [ ] **177.8 - Full runtime matrix requires prebuilt fixtures.**
  Several runtime/bridge groups failed because required binaries or
  fixtures were not prebuilt before `test-all`. Clarify the required
  `just build-test-fixtures` / service setup sequence or make `test-all`
  detect and report missing fixtures precisely.

### Test-All Runtime / E2E

- [ ] **177.9 - Runtime E2E failures need focused reruns.**
  The broad `test-all` tail reported 957 tests run: 876 passed, 79
  failed, 2 timed out, and 9 skipped. The harness summarized this as 29
  real failures out of 81 total failures/timeouts. Failures clustered in
  nano2nano, bridge, Zephyr, and service orchestration groups. Rerun
  these groups with required fixtures/services prebuilt and split real
  product bugs from host/setup fallout.

## Closed

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
- [ ] Full `test-all` rerun with PX4/ESP-IDF/PlatformIO/bridge fixtures
  prepared.

## Archive Rule

Archive this tracker only after:

- [ ] 177.2 and 177.3 move fully into their owning phases or close.
- [ ] 177.6 through 177.9 have owners and either close or move into more
  specific phase docs.
