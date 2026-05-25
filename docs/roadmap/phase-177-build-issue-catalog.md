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

- [x] **177.22 - ThreadX Cyclone participant init runtime trap.**
  Owner: Phase 177 runtime/Cyclone follow-up.
  Closed 2026-05-25. ThreadX RISC-V64 Cyclone fixtures build, link, boot,
  create the C talker publisher, and publish repeatedly without trapping.
  The 2026-05-24 manual two-QEMU probe boots ThreadX, initializes NetX Duo
  and BSD sockets, then reports `nros_support_init -> -1` on the listener;
  the talker traps with `mcause=0x7` at picolibc tinystdio
  `__file_str_put` (`mepc=0x80074270`, `mtval=0x10016c008`,
  `tinystdio/filestrput.c:44`). Phase 175 fixed the prerequisite
  allocation/link issues (`z_malloc`/`z_free`, C++ `new/delete`,
  Cyclone session-state allocation, and `stderr` binding). The runtime fix
  moves the Cyclone log buffer off ThreadX TLS, provides the board IPv4
  address to Cyclone, treats unsupported NetX socket options as
  unsupported instead of dereferencing TCP-only state, avoids the ThreadX
  socket waitset self-pipe path, disables the optional CDR stream
  optimization precompute on ThreadX, registers the C talker descriptor
  explicitly instead of relying on constructors, and uses Cyclone's `ddsrt`
  heap for transient publish samples. The focused verification was:
  `just cyclonedds threadx-cross-probe`, a sourced ROS rebuild of
  `riscv64_threadx_c_talker`, and a 20-second QEMU run showing
  `Publisher created for topic: /chatter` followed by `Published: 0..18`.
  The QEMU filter-dump pcap remains empty because the ThreadX Cyclone
  profile now disables multicast discovery; peer interop traffic is a
  separate follow-up tracked under 177.26, not the participant-init trap.

- [ ] **177.26 - ThreadX Cyclone peer interop / multicast discovery.**
  Owner: Phase 177 runtime/Cyclone follow-up. Split out of 177.22
  (participant-init trap, closed). The ThreadX RISC-V64 Cyclone C talker
  boots, creates the publisher, and publishes locally, but no two-node
  ThreadX↔ThreadX or ThreadX↔native RTPS exchange has been demonstrated.

  **2026-05-25 — discovery re-enabled, surfaced a byte-order defect.**
  - Flipped the ThreadX Cyclone profile from `<AllowMulticast>false</AllowMulticast>`
    to `spdp` (`packages/dds/nros-rmw-cyclonedds/src/session.cpp`). The
    board already enables IGMPv2 (`nx_igmp_enable`) and the virtio-net
    driver accepts all multicast on RX, so this is the right discovery
    path; data stays unicast.
  - Added a two-QEMU AF_UNIX-dgram e2e (shared L2, no slirp isolation):
    `packages/testing/nros-tests/tests/threadx_riscv64_qemu.rs::test_threadx_riscv64_cyclonedds_two_qemu_pubsub`.
    `#[ignore]`d until the bug below is fixed. Talker `10.0.2.40`/`:56`,
    listener `10.0.2.41`/`:57` (already distinct via each `config.toml`,
    applied through `startup.c` → `nros_board_set_network_config`).
  - One run confirmed SPDP discovery is now *attempted* (was fully
    suppressed before), but every write fails:
    `tev: ddsi_udp_conn_write to udp/1.0.255.239:7400 failed with retcode -12`.
    The listener also aborts at
    `nros_executor_register_subscription -> -1`.

  **Diagnosis — final, instrumentation-verified 2026-05-25.** The board's
  `nx_port.h` *does* define real `htonl`/`ntohl` (`__builtin_bswap32`), and
  `NX_IP_CLASS_D_TYPE = 0xE0000000` (`nx_api.h:991`); instrumentation of the
  two-QEMU dgram run pinned **two** real defects in the ThreadX ddsrt port
  (`src/ddsrt/src/sockets/threadx/socket.c`), both since fixed:

  1. **IGMP join byte order.** `setsockopt(IPPROTO_IP, IP_ADD_MEMBERSHIP)`
     returned `EINVAL`. Cyclone hands the multicast group to the BSD layer
     in *host* byte order (`maddr=0xefff0001`) while NetX's class-D check
     `imr_multiaddr & ntohl(NX_IP_CLASS_D_TYPE)` expects *network* order
     (`nxd_bsd.c:7124`); `0xefff0001 & 0x000000e0 = 0 ≠ 0xe0` → reject. The
     interface address (`0x2902000a`) already arrived network-ordered. Fix:
     normalise `imr_multiaddr` to network byte order in `ddsrt_setsockopt`.
  2. **Multi-iovec datagram send.** SPDP/RTPS `ddsi_udp_conn_write` failed
     with `-12` (`EDESTADDRREQ`/`ENOTCONN`). RTPS messages are multi-iovec
     (header + submessages), so `ddsrt_sendmsg` fell into the per-iov
     `nx_bsd_send` loop, which is a *connected* send with **no destination**
     — wrong for connectionless UDP. Fix: when a destination is present,
     coalesce the iovecs into one buffer and `nx_bsd_sendto` once (also
     applying the multicast byte-order swap to the destination).

  Both fixes are committed in the cyclonedds fork (`NEWSLabNTU/cyclonedds`
  branch `nano-ros/zephyr-nsos-patches`, local commit `e8ce7315`). **Not yet
  pushed / superproject pointer not bumped** — the agent is not permitted to
  push the external fork; a maintainer must push it and bump the submodule
  pointer. The earlier byte-order/multicast-egress write-ups in this item
  were partially wrong (the diagnosis zig-zagged); this block supersedes
  them.

  **Verified.** With the fixes, the ThreadX RISC-V64 Cyclone C talker joins
  the SPDP group and publishes 24/24 with **zero** `conn_write` errors over
  a two-QEMU AF_UNIX-dgram link. Multicast discovery TX is working.

  **Listener subscription: fixed (177.28).** The listener's
  `register_subscription -> -1` was a missing CycloneDDS type-descriptor
  registration in the listener binary, not an executor/arena issue — see
  177.28. The listener now registers and reaches `Waiting for messages...`.

  **Locator byte order — root cause found + fixed (local submodule commit
  `5558c6ae`, on top of `e8ce7315`; fork push pending — agent is hard-blocked
  from pushing the external cyclonedds fork).** The reversed locators traced
  to a single ThreadX defect: `ddsrt_sockaddrfromstr`'s `WITH_THREADX` branch
  (`src/ddsrt/src/sockets.c:208`) wrote `sin_addr.s_addr` in **host** byte
  order, so every parsed locator — including the SPDP multicast group — came
  out reversed (Cyclone logged `SPDP MC: udp/1.0.255.239`). `htonl` it. The
  ThreadX ifaddrs port (`ifaddrs.c:78`) had the same bug for the board's
  interface address (host-order → byte-swapped advertised unicast locator);
  `htonl` addr/netmask/broadcast there too. With the sources network-ordered,
  the earlier `socket.c` band-aids (imr_multiaddr swap, multicast-dest swap)
  were **removed** (they would double-swap); the multi-iovec `sendmsg`
  coalescing stays. Verified after the fix: `SPDP MC: udp/239.255.0.1`, IGMP
  join + SPDP TX work band-aid-free, SPDP crosses both ways, advertised
  unicast locator is `udp/10.0.2.41`, and the gateway-ARP churn is gone.

  **Current blocker — NetX multicast RX not delivered to Cyclone.** With all
  locators correct, the listener still logs **no incoming-packet trace** at
  `finest` verbosity (`recv` and `recvUC` threads start; no SPDP is ingested,
  no proxy participant is created) even though the pcap shows the peer's SPDP
  arriving on `net0`. So NetX Duo is not surfacing the joined multicast
  datagrams to Cyclone's `recv` thread — likely `nx_bsd_select` not reporting
  the multicast-joined socket readable, or the multicast receive socket
  bind/port wiring. This is NetX-multicast-RX-port work, distinct from the
  (now fixed) TX/locator byte-order issues.

  **Next.**
  1. Maintainer: push cyclonedds `5558c6ae` to `nano-ros-fork`
     (`nano-ros/zephyr-nsos-patches`) and bump the submodule pointer. (The
     177.28 listener descriptor fix is already in the nano-ros repo.)
  2. Diagnose NetX multicast RX delivery: confirm whether `nx_bsd_select`
     flags the SPDP multicast socket readable on datagram arrival and whether
     `ddsrt_recvmsg` returns the data; fix the RX/select path so Cyclone
     ingests the peer SPDP → proxy participant → SEDP → data.
  3. Re-run the `#[ignore]`d test (`--ignored`); only a decoded sample on the
     listener proves two-node RTPS.

- [x] **177.27 - ThreadX-Linux C/C++ CycloneDDS fixtures fail to build.**
  Found 2026-05-25 while staging fixtures for 177.9.H; closed 2026-05-25.
  Was not one bug but four layered gaps in the never-completed threadx-linux
  cyclonedds fixture path, each surfacing only after the previous was fixed:
  1. **Configure** — `nros_rmw_cyclonedds_generate_from_msg` couldn't find
     `msg_to_cyclone_idl.py`; the `build-fixture-extras` recipe in
     `just/threadx-linux.just` never passed `-DNROS_RMW_CYCLONEDDS_MSG_TO_IDL=`
     (native.just / cyclonedds.just both do). Added it.
  2. **Stale-dir retry trap** — `nros_cmake_fixture_build`
     (`scripts/build/fixture-matrix.sh`) wrote its `.sig` even when `cmake`
     configure failed, so a retry with the env fixed saw a matching signature,
     skipped reconfigure, and ran `cmake --build` on a build dir with no
     generated build system → `gmake: Makefile: No such file`. Now writes the
     signature only after a successful configure (general fix; benefits every
     platform's fixture build).
  3. **ddsc link** — the recipe never passed
     `-DCMAKE_PREFIX_PATH=build/install`, so `find_package(CycloneDDS)` didn't
     resolve and the C++ fixtures failed to link (`undefined dds_qset_* /
     ddsrt_* / dds_stream_*`). Added it (native.just passes it too).
  4. **C++ runtime link for C apps** — the **C** examples link the C++
     cyclonedds backend, but CMake selected the C linker driver and failed on
     `operator new/delete` / `std::nothrow`. (Native gets the C++ driver via
     automatic link-language propagation; the threadx whole-archive transitive
     path loses it.) `nano_ros_link_rmw` (`cmake/NanoRosLink.cmake`) now forces
     `LINKER_LANGUAGE CXX` on the target whenever the C++ `cyclonedds` backend
     is linked — idempotent for C++ apps and hosts where it already works.
  Verified 2026-05-25: `just threadx_linux build-fixture-extras` exits 0 and
  produces all 24 c/cpp fixtures (zenoh + cyclonedds × talker/listener/
  service-server/service-client/action-server/action-client). Runtime
  `rtos_e2e` cyclonedds cases were not run here (build-only scope); they fall
  under 177.9.F. Sibling of 177.24 (Zephyr CycloneDDS) but distinct root
  causes.

- [x] **177.28 - ThreadX Cyclone listener: `register_subscription` fails in
  the nano-ros executor before backend create.**
  **Closed 2026-05-25.** Root cause: the C **listener** never registered the
  CycloneDDS `std_msgs/Int32` type descriptor. The backend calls a *weak*
  `nros_rmw_cyclonedds_register_app_descriptors` (no-op default,
  `vtable.cpp:134`); the **talker** overrides it via `src/cyclonedds_app.c`
  (→ `register_Int32_0()`), but the listener's `CMakeLists.txt` had no
  cyclonedds source block, so `find_descriptor("std_msgs::msg::dds_::Int32_")`
  returned null and `subscriber_create` failed `UNSUPPORTED` → executor
  `Transport` error → C `-1`. (The earlier "Rust executor / arena" guess was
  wrong — arena alloc is *after* the backend create, which was reached.) Fix:
  add `examples/qemu-riscv64-threadx/c/listener/src/cyclonedds_app.c` and the
  `if(NROS_RMW STREQUAL "cyclonedds") list(APPEND _app_sources …)` block,
  mirroring the talker. Verified: the listener now registers the subscription
  and reaches `Waiting for messages...`. The remaining two-node *data*
  exchange is tracked under 177.26 (see the SPDP/locator finding there).

  *Original investigation (superseded by the close above):*
  Owner: Phase 177 runtime/executor follow-up. Split out of 177.26 (which
  fixed the multicast discovery TX path). **Pre-existing** — reproduced on
  the first ThreadX RISC-V64 Cyclone listener run before any 177.26 change,
  and orthogonal to multicast/Cyclone.

  **Symptom.** The C listener aborts at
  `nros_executor_register_subscription(&app.executor, &app.subscription, NROS_EXECUTOR_ON_NEW_DATA) -> -1`
  (`examples/qemu-riscv64-threadx/c/listener/src/main.c:68`). The talker
  (publisher only) is unaffected — it registers no subscription.

  **Localised.** Instrumentation of the Cyclone backend showed
  `subscriber_create` (`packages/dds/nros-rmw-cyclonedds/src/subscriber.cpp`)
  is **never called**, and no socket/`setsockopt`/`bind` op runs for the
  reader. So the `-1` originates in the nano-ros Rust executor
  `register_subscription_raw_with_qos_sized::<MESSAGE_BUFFER_SIZE>`
  (`packages/core/nros-c/src/executor.rs:771`) *before* it reaches the RMW
  backend. The C-level capacity guard (`handle_count >= max_handles`,
  `executor.rs:718`) is not the cause — the listener registers its first of
  four handles. The likely cause is the executor arena allocation for the
  subscription entry + `MESSAGE_BUFFER_SIZE` receive buffer failing, i.e. a
  `NROS_EXECUTOR_ARENA_SIZE` / `MESSAGE_BUFFER_SIZE` mismatch in the ThreadX
  Cyclone listener build config (`.cargo`/CMake-injected env), not a
  Cyclone/NetX defect.

  **Next.**
  1. Confirm the failing arm in `register_subscription_raw_with_qos_sized`
     (arena reserve vs another precondition) — instrument the Rust executor
     or compare the arena size against `size_of` the subscription entry +
     `MESSAGE_BUFFER_SIZE`.
  2. If arena sizing: raise `NROS_EXECUTOR_ARENA_SIZE` for the ThreadX
     Cyclone listener fixture (and document the per-RMW requirement).
  3. Unblocks the 177.26 two-QEMU e2e
     (`test_threadx_riscv64_cyclonedds_two_qemu_pubsub`, currently `#[ignore]`d).

- [x] **177.29 - ThreadX-RV64 Cyclone fixtures fail to link: GCC slim-LTO
  `libddsc.a` vs rust-lld.** Surfaced by Phase 179.G; closed 2026-05-25.
  `NROS_THREADX_RV64_CYCLONEDDS_FIXTURES=1 just threadx_riscv64 build-fixtures`
  failed linking the C/C++ Cyclone fixtures with unresolved `dds_*` /
  `ddsrt_*` / `ddsi_*` from `libnros_rmw_cyclonedds.a`. Distinct root cause
  from 177.27 (threadx-**linux**, which was a missing `CMAKE_PREFIX_PATH` /
  `find_package`): the cross `build/cyclonedds-threadx-rv64-install/lib/
  libddsc.a` *was* on the link line (whole-archived) and GNU `nm` saw all
  498 `dds_*` symbols, but `llvm-nm` saw **zero** — only `__gnu_lto_slim`.
  The archive held GCC slim-LTO objects (GIMPLE bytecode, not machine code);
  rust-lld (the linker for ThreadX examples, via
  `cmake/toolchain/riscv64-lld-wrapper.sh`) cannot consume GCC LTO objects
  without GCC's LTO plugin, so every symbol was undefined even under
  `--whole-archive`. The cross-probe already set `-fno-lto` +
  `ENABLE_LTO=OFF` (2026-05-24), but the build dir kept a stale
  `ENABLE_LTO:BOOL=ON` cache and an incremental `--mode build` reused it.
  Fix: `scripts/cyclonedds/threadx-cross-probe.sh` now wipes the build dir
  for a clean reconfigure whenever the cached LTO setting is not
  `ENABLE_LTO:BOOL=OFF` (`941353aa4`). Verified: after a clean rebuild
  `llvm-nm` resolves the symbols, all four C/C++ Cyclone fixtures link to
  RISC-V ELF, and `NROS_THREADX_RV64_CYCLONEDDS_FIXTURES=1 just
  threadx_riscv64 build-fixtures` exits 0. Runtime is still gated by 177.26
  (multicast) — the talker boots, creates the publisher, and publishes; the
  two-node data plane is the 177.26 / 177.28 work.

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

- [x] **177.24 - Zephyr CycloneDDS fixtures fail after Cyclone setup.**
  Closed 2026-05-25 — already fixed by `4b1b0723d` ("test: replace fixed
  sleeps with readiness waits"), which the 2026-05-25 recheck below predated.
  The recorded blocker was `internal.hpp::platform_now_ms()` /
  `platform_sleep_ms()` falling into the `#else` branch that uses
  `std::chrono::steady_clock` / `std::this_thread::sleep_for`, which Zephyr's
  minimal `native_sim` C++ shim does not expose. `4b1b0723d` added explicit
  `NROS_PLATFORM_ZEPHYR || __ZEPHYR__` branches that route through the C
  shim (`nros_platform_time_ns()` / `nros_platform_sleep_ms()`) and confined
  the `<chrono>` / `<thread>` includes to the non-RTOS `#else`. Because the
  Zephyr fixtures compile nros-rmw-cyclonedds through the Zephyr toolchain
  (`__ZEPHYR__` is always defined there), the embedded branch now engages and
  no chrono shim is pulled. Verified 2026-05-25 by building the CycloneDDS
  talker fixtures for all three languages — they compile and link clean:
  `NROS_ZEPHYR_FIXTURE_FILTER='build-cpp-talker-cyclonedds' just zephyr build-fixtures`
  then `NROS_ZEPHYR_FIXTURE_FILTER='build-(rs|c)-talker-cyclonedds' just zephyr build-fixtures`
  both report "Zephyr test fixtures built successfully" (nros-rmw-cyclonedds
  `session/sertype_min/publisher/subscriber/service/vtable.cpp` all build).
  Original recheck context retained: `just cyclonedds doctor` passes and the
  host artifacts exist at `build/install/bin/idlc` + `lib/libddsc.so`. This
  unblocks the CycloneDDS slice of 177.9.F (Zephyr E2E runtime).

### Test-All Runtime / E2E

- [x] **177.9 - Runtime E2E failures need focused reruns.**
  Closed 2026-05-25 — all groups 177.9.A–H are resolved (the last,
  177.9.F's cpp/xrce action feedback, fixed in `57ebb8182`).
  The 2026-05-22 `test-all` rerun reported 960 tests run: 911 passed, 49
  failed, and 9 skipped after `just setup` and `just build-test-fixtures`
  both passed. The remaining failures are grouped below so owners can
  close them independently. Newer focused fixes closed 177.19 and 177.20;
  rerun these groups with required fixtures/services prebuilt and split
  remaining product bugs from host/setup fallout.

  **Rerun-failed workflow (added 2026-05-25).** The full → list → fix →
  rerun-failed loop is now a recipe, reusing the existing JUnit + nextest
  run-profile infra:
  1. `just test-all` — full coverage; writes `target/nextest/default/junit.xml`
     and prints `_test-summary` (real failures vs `[SKIPPED]` env-skips).
  2. Debug/fix the failures.
  3. `just test-failed` — reruns **only** the real (non-`[SKIPPED]`) failures
     from that JUnit report. `scripts/test/failed-filterset.py` turns each
     failed `<testcase>` into `(binary_id(=<classname>) & test(=<name>))`,
     unioned into one nextest `-E` filterset; the rerun uses the same
     `nros_cargo_nextest_args` cargo profile, run-profile, and per-platform
     groups (retries/serialization) as the full run, and overwrites the
     JUnit report with the subset result — so repeating step 3 naturally
     shrinks the set until `test-failed` reports all clear.
  Notes: `test-failed` reruns from whatever the latest JUnit holds, so run a
  full `test-all` (or a scoped `just <plat> test`) first; `[SKIPPED]`
  environment-skips are never rerun (they need the missing prerequisite, not
  a code fix). Fixture-dependent groups still need `just build-test-fixtures`
  / SDK env before the rerun will pass.

  **Fixtures preflight (added 2026-05-25).** To stop the common "forgot to
  build fixtures → whole matrix mass-fails with `Binary not found`" trap,
  `build-test-fixtures` now stamps `target/nextest/.fixtures-built` on
  success, and `just test-all` gains a `_require-fixtures` preflight that
  fast-fails (~1 s, before any build) with a `just build-test-fixtures` hint
  when the stamp is absent. Bypass with `NROS_SKIP_FIXTURE_CHECK=1` when
  fixtures were built another way (e.g. scoped `just <plat> build-fixtures`).
  Only `test-all` is gated — `test`/`test-integration` stay ungated for
  partial-fixture quick iteration.

  **Staleness detection (added 2026-05-25, content-hash).** Beyond presence,
  C/C++ fixture cells now carry a content-hash input signature so an *edited
  but not-rebuilt* source is caught (the harness consumes prebuilt binaries,
  so a stale binary would otherwise be used silently). `nros_cmake_fixture_build`
  (`scripts/build/fixture-matrix.sh`, the chokepoint every C/C++ cell routes
  through) writes `<build-dir>/.nros-fixture.inputsig` on successful build =
  `sha1(shared-inputs + cell tracked sources)`, where shared-inputs =
  `git ls-files packages/ Cargo.lock rust-toolchain.toml cmake/` + the
  third-party submodule gitlinks (SDK pins). `test-all` runs a
  `_check-fixtures-stale` preflight that recomputes each cell's signature and
  **warns (non-fatal)** with the list of stale cells + a
  `just build-test-fixtures` hint (incremental — only changed cells rebuild).
  Content-hash (not mtime) so `git checkout`/`touch` don't trigger false
  staleness; granularity is per cell (a cpp-only edit flags only cpp cells; a
  shared-crate edit flags all — correct, they all link it). Honest limits: the
  shared-input set is coarse (all of `packages/`) so any crate edit flags every
  cell (safe over-invalidation); it's a heuristic, not the cargo/cmake
  dependency graph. Bypass everything with `NROS_SKIP_FIXTURE_CHECK=1`.

  **Rust cells (added 2026-05-25, reuse cargo).** Rather than a custom hash,
  rust fixtures delegate staleness to cargo's own fingerprint:
  `scripts/test/rust-fixture-stale.sh` runs `cargo build <fixture-profile>
  --message-format=json` per built rust example dir
  (`examples/*/rust/* with target/<profile>/`); a `"fresh":false` artifact
  means cargo had to rebuild it (= it was stale). Because `cargo build` is a
  no-op when fresh and incremental when stale, this both **detects and
  self-heals** rust fixtures (C/C++ only warns — they need the SDK/CMake env
  to rebuild). `_check-fixtures-stale` runs the per-dir probes in parallel and
  reports which were rebuilt. Default-feature build (matches
  `just <plat> build-examples`); cyclonedds-rust cells go through the CMake
  path and are covered by the `.inputsig` hash instead. Verified: a real run
  caught + rebuilt 29 stale rust fixtures. Cost: ~no-op when all fresh
  (parallel), up to an incremental rebuild when many are stale (e.g. after a
  core-crate edit) — bypass with `NROS_SKIP_FIXTURE_CHECK=1`.

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

- [x] **177.9.C - Native C/XRCE runtime.**
  Closed 2026-05-25. Initial focused rerun failed because the native C
  XRCE fixtures were not prebuilt:
  `examples/native/c/{talker,listener}/build-xrce/c_{talker,listener}`.
  After `just native build-fixtures`, the runtime-only C/XRCE group passed:
  `just native test-c-xrce verbose` reported 5 tests run, 5 passed, 0
  skipped.
  - [x] `c_xrce_api::test_c_xrce_listener_starts`
  - [x] `c_xrce_api::test_c_xrce_talker_listener_communication`
  - [x] `c_xrce_api::test_c_xrce_talker_starts`

- [x] **177.9.D - QEMU RTIC and QEMU zenoh/serial runtime.**
  Closed 2026-05-25. Not a runtime bug — every failure was a missing
  prebuilt fixture, and the fixture build itself was broken. The
  qemu-arm-baremetal examples wire `std_msgs` / `builtin_interfaces`
  through `[patch.crates-io] -> generated/` in their `.cargo/config.toml`,
  but `just qemu build-fixtures` ran `cargo build` without first running
  `nros generate-rust`, so cargo could not load the (gitignored) generated
  crates. The plain `listener`/`talker` build failed on the absent
  `generated/builtin_interfaces`, and `parallel --halt now,fail=1` then
  killed every in-flight fixture build — so none of the RTIC/serial/large-msg
  binaries the 177.9.D tests resolve were ever staged. Fixed by adding a
  codegen step (gated on `package.xml`) before `cargo build` in
  `just/qemu-baremetal.just::build-fixtures`, mirroring the native recipe's
  `ensure_native_rust_generated`. After `just qemu build-fixtures`, all six
  tests pass:
  `cargo nextest run -p nros-tests --no-fail-fast -E '(binary(emulator) and (test(test_qemu_rtic_pubsub_e2e) or test(test_qemu_rtic_service_e2e) or test(test_qemu_rtic_action_e2e) or test(test_qemu_rtic_mixed_priority_pubsub_e2e) or test(test_qemu_serial_pubsub_e2e))) or (binary(large_msg) and test(test_qemu_zenoh_large_publish))'`
  → `6 passed`.
  - [x] `emulator::test_qemu_rtic_action_e2e`
  - [x] `emulator::test_qemu_rtic_mixed_priority_pubsub_e2e`
  - [x] `emulator::test_qemu_rtic_pubsub_e2e`
  - [x] `emulator::test_qemu_rtic_service_e2e`
  - [x] `emulator::test_qemu_serial_pubsub_e2e`
  - [x] `large_msg::test_qemu_zenoh_large_publish`

- [x] **177.9.E - XRCE runtime.**
  Closed 2026-05-25. The XRCE harness now passes the canonical
  `NROS_LOCATOR` and enables `RUST_LOG=info` so `wait_for_output_*`
  observes the current env-logger markers. The service/action assertions
  were aligned with the current example output, and the multi-message
  test now waits for real `Received:` counts instead of a stale summary
  marker. Runtime fixes: the XRCE talker drives IO after each manual
  publish so repeated samples flush, and the action server periodically
  drives IO around goal accept/status/feedback/result work instead of
  relying on a typed action loop with no executor spin.
  Verification: `cargo nextest run --cargo-profile nros-fast-release -p
  nros-tests --no-fail-fast --test xrce` (14 passed, 0 skipped).
  - [x] `xrce::test_xrce_action_fibonacci`
  - [x] `xrce::test_xrce_multiple_messages`
  - [x] `xrce::test_xrce_service_request_response`
  - [x] `xrce::test_xrce_talker_listener_communication`

- [x] **177.9.F - Zephyr native/cross E2E runtime.**
  Closed 2026-05-25 — all 18 subtests pass: Zenoh/cpp (12/12), CycloneDDS
  `dds` group (15/15), and the full XRCE subset, after the session-key
  collision fix (`5b9ad9aab`) and the action-feedback double-CDR-header fix
  (`57ebb8182`).
  Focused rerun on 2026-05-25:
  `NROS_ZEPHYR_BUILD_ROOT=/home/aeon/repos/nano-ros/build/zephyr-workspace-builds
  cargo nextest run --cargo-profile nros-fast-release -p nros-tests
  --no-fail-fast --test zephyr` with the 177.9.F Zenoh test filter.
  Result: 11/11 Zenoh tests passed after rebuilding native_sim fixtures
  with the shared NSOS overlay and per-language/per-role Zenoh locator
  Kconfig overrides. The prior `eth_posix: Cannot create zeth (0)`
  failure is gone; fixture logs report `Network ready (NSOS - host
  kernel sockets)`. C++ action also now emits the same `[OK]` success
  marker that the test harness waits for.

  XRCE follow-up on 2026-05-25 moved the Agent prerequisite into
  `just zephyr setup` and `just zephyr doctor`, then rebuilt the XRCE
  fixture subset with the NSOS overlay. The first live-agent run exposed
  stale fixture wiring: C and C++ XRCE tests start agents on per-language
  ports, but `just zephyr build-fixtures` was compiling every XRCE
  fixture against the default port 2018. The fixture matrix now passes
  `CONFIG_NROS_XRCE_AGENT_PORT` for each `(language, role)` cell:
  Rust 2018/2028/2038, C 2118/2128/2138, and C++ 2218/2228/2238.
  After rebuilding, the focused XRCE subset initially ran 7 tests: 2 passed
  and 5 failed (runtime/backend issues, not setup fallout). **Update
  2026-05-25:** after the incoming `cf34366fd` ("fix: wire Zephyr XRCE
  setup") landed and the XRCE fixtures were rebuilt with the NSOS overlay,
  the XRCE pub/sub+service subset reached **6/7** and
  `test_zephyr_xrce_cpp_talker_listener` was then fixed (session-key
  collision, `5b9ad9aab`). The last XRCE failure,
  `test_zephyr_xrce_cpp_action_e2e` (`feedback=0` — cpp action feedback
  double-CDR-header), is now also fixed (`57ebb8182`, see below), so the
  **full XRCE subset passes**. Combined with the Zenoh/cpp subset (12/12)
  and the CycloneDDS (`dds`) subset (**15/15**: `binary(zephyr) and
  test(dds)`, boots + c/cpp/rs action e2e on fresh NSOS fixtures, see the
  CycloneDDS slice below), Zephyr native/cross E2E runtime is **green across
  Zenoh, XRCE, and CycloneDDS — 18/18**.
  - [x] `test_bidirectional_native_zephyr_e2e` passes.
  - [x] `test_native_server_zephyr_client` passes.
  - [x] `test_native_talker_to_zephyr_cpp_listener` passes.
  - [x] `test_native_to_zephyr_e2e` passes.
  - [x] `test_zephyr_action_e2e` passes.
  - [x] `test_zephyr_cpp_action_server_to_client_e2e` passes.
  - [x] `test_zephyr_cpp_service_server_to_client_e2e` passes.
  - [x] `test_zephyr_cpp_talker_to_listener_e2e` passes.
  - [x] `test_zephyr_cpp_talker_to_native_listener` passes after the
        2026-05-25 count-wait fix: it waited for only 1 "Received:" but
        asserted `>= 2`, failing deterministically once fixtures were staged;
        now waits for 2.
  - [x] `test_zephyr_to_native_e2e` passes.
  - [x] `test_zephyr_talker_to_listener_e2e` passes.
  - [x] `test_zephyr_xrce_c_talker_listener` passes with `just zephyr setup`
        provided Agent and fixtures rebuilt against port 2118.
  - [x] `test_zephyr_xrce_rust_talker_listener` passes with `just zephyr setup`
        provided Agent and fixtures rebuilt against port 2018; the harness now
        accepts the Rust fixture's `Received[n]:` log format.
  - [x] `test_zephyr_xrce_cpp_service_e2e` — passes after the incoming
        `cf34366fd` ("fix: wire Zephyr XRCE setup") landed and the XRCE
        fixtures were rebuilt (2026-05-25 rerun).
  - [x] `test_zephyr_xrce_cpp_action_e2e` — **FIXED 2026-05-25 (double CDR
        header on the feedback path).** Two independent investigations pinned
        the same root cause; the fix landed is the shared-path strip, after
        verifying it does NOT regress Cyclone (see "Fix + Cyclone verification"
        below). With the session-name fix the goal/result round-trip is `[OK]`,
        but the client logged `feedback=0`. Fully traced with NSOS fixtures +
        a runnable agent (all instrumentation since reverted):
        * The full path works up to the C++ deserializer: `xrce_topic_callback`
          fires for the feedback DataReader (`oid=7 len=72`), the C-side
          `xrce_subscriber_try_recv_raw` returns it (`dr=7 count=1 len=72`),
          `action_client_raw_try_process` drains it, `cpp_feedback_trampoline`
          **stashes** it, and `nros_cpp_action_client_try_recv_feedback`
          **reads the stash (STASH-HIT)** and returns OK. So the wiring +
          dispatch are fine — the doc's earlier "not dispatched / wrong ring"
          guesses were wrong.
        * The break is `FeedbackType::ffi_deserialize` failing in the C++
          `try_recv_feedback` wrapper (`action_client.hpp:219`) → `Result(Error)`
          → the example's `while(try_recv_feedback)` skips logging → `feedback=0`.
        * **Why it fails — double CDR header (XRCE-specific).** A byte dump of
          the arena feedback payload showed `feedback_data[0..8] = 00 01 00 00
          0a 00 00 00`: it **already begins with `CDR_LE_HEADER`**. But
          `cpp_feedback_trampoline` (`action.rs`) and the direct path in
          `try_recv_feedback` both **prepend `CDR_LE_HEADER` again**, so the
          deserializer reads the second header as the first field → fails.
        * **It is NOT the common trampoline** — Zephyr **Cyclone** cpp action
          feedback works (verified: `Feedback: length=10`). For Cyclone the
          arena payload is **raw fields** (no header), so the trampoline's
          prepend is correct. XRCE delivers the feedback payload **with** an
          inner CDR header that Cyclone doesn't, so the same prepend
          double-frames it. The asymmetry (XRCE action feedback payload carries
          an extra CDR encapsulation vs Cyclone) is the bug; the fix belongs on
          the XRCE feedback framing, NOT the shared `cpp_feedback_trampoline`
          (changing that would break the working Cyclone path).
        * Secondary: feedback is volatile and the server bursts all 10 in a
          ~1 ms synchronous callback, so only ~1–2 reach the client even when
          deserialization is fixed. Test needs `feedback >= 1`, so one
          surviving + correctly-deserialized frame suffices; pacing the server
          would make it robust.
        **Localized fully 2026-05-25** by dumping `feedback_buffer[0..28]` on
        both backends (instrumentation reverted):
        * Server publish is **identical** for both — `publish_feedback_raw`
          (`action_core.rs`) embeds `[outer CDR(4) + GoalId(20) + feedback_cdr]`
          where `feedback_cdr` is the C++ `ffi_serialize` output and **keeps its
          own CDR header** (`SRV feedback_cdr [0..8] = 00 01 00 00 04 00 00 00`
          on both XRCE and Cyclone). So the published feedback message is a
          *nested* CDR: `[CDR + GoalId + (CDR + fields)]`.
        * Client receive **differs**: XRCE delivers the bytes **verbatim** —
          `feedback_buffer[0..28] = 00 01 00 00 | <GoalId 20> | 00 01 00 00`
          (len 72), so `[24..]` is `[inner CDR + fields]`. Cyclone's backend
          **reconstructs the message flat** — `… | <GoalId 20> | 0a 00 00 00`
          (len 68), so `[24..]` is `[fields]` (no inner header). The flattening
          is Cyclone-specific code: `packages/dds/nros-rmw-cyclonedds/src/
          subscriber.cpp:185-197` (rebuilds `[CDR + GoalId-len + fields]` from
          the DDS dynamic sample) paired with `publisher.cpp::
          publish_fibonacci_feedback`.
        * So the arena offset-24 + `cpp_feedback_trampoline` prepend assume the
          **flat** layout Cyclone produces; XRCE's verbatim nested layout
          double-frames.

        **Fix LANDED 2026-05-25 (`57ebb8182`) — FFI-level strip, verified on
        both backends.** Instead of the broader publish_feedback_raw +
        trampoline + Cyclone-bridge refactor, the strip was applied one layer
        up, at the FFI boundary: `nros_cpp_action_server_publish_feedback`
        (`nros-cpp/src/action.rs`) now strips the C++ serializer's CDR header
        before `publish_feedback_raw`, exactly mirroring
        `nros_cpp_action_server_complete_goal` (results already do this). The
        published feedback message becomes the flat `[outer CDR + GoalId +
        fields]` for *every* backend, so the offset-24 slice + the
        `cpp_feedback_trampoline` prepend frame it once. Because this is the
        *shared* path, the cross-RMW concern was tested directly rather than
        assumed: with the strip, BOTH `test_zephyr_xrce_cpp_action_e2e`
        (feedback `length=7`, `length=10`; cpp/xrce zephyr subset 9/9) **and**
        `test_zephyr_dds_cpp_action_e2e` (Zephyr CycloneDDS) **pass**. So the
        feared Cyclone regression did not occur — Cyclone's flatten-on-receive
        path (`subscriber.cpp`) consumes the now-already-flat wire fine, and
        its result path already relied on fields-only.

        The broader refactor described above (strip in `publish_feedback_raw`,
        drop the trampoline prepend, simplify Cyclone's `subscriber.cpp`
        rebuild) remains a valid cleanup if one canonical framing is preferred;
        the FFI strip is the minimal landed fix. **Parallel-work note:** this
        item was investigated concurrently from two directions (the
        localization above + the landed FFI fix) — fold the FFI strip into the
        broader refactor if that path is taken. Secondary follow-up: pace the
        server's 10-feedback burst (volatile, ~1 ms) so ≥1 reliably survives.
  - [x] `test_zephyr_xrce_rust_service_e2e` — passes (same fix + rebuild);
        the earlier `Transport(ConnectionFailed)` is gone.
  - [x] `test_zephyr_xrce_rust_action_e2e` — passes (same fix + rebuild).
  - [x] `test_zephyr_xrce_cpp_talker_listener` — **FIXED 2026-05-25 (root
        cause: XRCE session-key collision).**

        **RESOLUTION.** Every Zephyr C++ example calls the 2-arg
        `nros::init(addr, domain)`, whose wrapper defaults the session name to
        `"nros_cpp"` (`node.hpp::init` → `init(..., "nros_cpp")`). The XRCE
        client key is `hash_session_key(session_name)` (`session.c`), so the
        cpp talker AND cpp listener registered with the **same client key** on
        the same Agent. XRCE-DDS treats same-key connections as one client and
        **resets the existing session when the second connects**, dropping the
        listener's DataReader → `try_recv` returns 0. The fix: pass a distinct
        session name per process (its node name) to the 3-arg
        `nros::init(addr, domain, session_name)` in
        `examples/zephyr/cpp/{talker,listener,service-server,service-client,
        action-server,action-client}/src/main.cpp`. Verified: the standalone
        repro went 0 → 15/16 received and 3 boots → 1 boot; the nextest
        `test_zephyr_xrce_cpp_talker_listener` now **passes**, and
        `test_zephyr_xrce_cpp_service_e2e` (which had the same collision —
        server+client both `"nros_cpp"`) now **passes** too (it failed when
        reverted to the shared name, passes with distinct names). The
        **reboot loop** documented below was a *symptom* of the collision
        churn amplified under nextest load — the gdb standalone reproduced the
        `receives 0` with **no reboot** (ran clean to iter 457), proving the
        reboots were not the disease. **Footgun follow-up:** the 2-arg
        `nros::init` default `"nros_cpp"` collides for any two cpp XRCE nodes
        on one Agent; consider making the default client key unique per
        process at the API level.

        --- Original investigation history (the reboot saga; superseded by the
        resolution above) ---
        Talker publishes 1..N on port 2218; the C++
        listener stays at "Waiting for messages" and receives 0. Root cause
        isolated by differential:
        * The C++ *zenoh* pubsub test passes with the same `sub.try_recv()`
          poll API → not a test/timing bug.
        * `test_zephyr_xrce_c_talker_listener` (C, same XRCE backend +
          `subscriber.c` ring buffer) passes → the backend's poll path
          (`xrce_topic_callback` → ring → `xrce_subscriber_try_recv_raw`)
          works.
        * C++ XRCE service + action pass → the C++ executor spin pumps the
          XRCE session fine.
        So the failure is specific to the **C++ pubsub DataReader** receive
        over XRCE. nros-cpp's subscription API is poll-only
        (`try_recv`/`borrow`; no callback-registration variant like the Rust
        `executor.register_subscription` the working Rust listener uses), so
        there is no example-side workaround — the fix must be runtime-side.

        **Root cause pinned 2026-05-25** via temporary `printf` traces in the
        XRCE C backend (`session.c` / `subscriber.c` / `publisher.c`, since
        reverted). Agent-side `-v6` was unusable (a manual `MicroXRCEAgent`
        exits 144 under the sandbox; the nextest-spawned agent is SIGKILLed
        before flush), so the diagnosis is firmware-side:
        * Topic/type match perfectly — pub and sub both register
          `rt/chatter` / `std_msgs::msg::dds_::Int32_`.
        * The talker's `uxr_buffer_topic` writes succeed, and the **listener's
          `xrce_topic_callback` DOES fire** (`oid.id=4 type=6 len=8`) — data
          reaches the listener's input stream.
        * But the callback finds **every `st->subscriber_slots[i]` with
          `active=0, dr_id=0`** even though the subscription was registered
          (`dr_id=4, active=1`) on the *same* `st` pointer.
        * Session-lifecycle trace showed multiple `create_session` calls per
          listener process (each `calloc`s a fresh `xrce_session_state_t`,
          zeroing `subscriber_slots`), so the slot registered in one generation
          was gone when data arrived → callback iterates a zeroed array → no
          match → ring empty → `try_recv` returns nothing.

        **Actual root cause (the repeated `create_session` is a symptom):
        the C++ XRCE listener firmware reboots in a loop.** Counting Zephyr
        boot banners in each captured firmware's output: the **talker boots
        once** (stays up and publishes 1..N) while the **listener boots 3×**
        in the ~20 s window. Each reboot re-runs `main` → `init` (new session
        `create_session`) → `create_subscription` (re-register) → "Waiting for
        messages", and some boots open the session but reboot before
        re-registering, so inbound data (the agent still holds the old
        `dr_id=4`) hits a slot-less session → `NO-MATCH`. The listener never
        stays alive long enough to deliver a message to the test. No crash
        dump / FATAL / fault line is printed before the reboots (silent
        restart). The C listener and the C++ talker do **not** reboot, so this
        is specific to the **C++ XRCE listener** runtime path.

        **Fix target:** find why the C++ XRCE listener restarts (a fault in the
        spin/`try_recv` receive loop that native_sim turns into a silent
        re-boot, or an unhandled exit). Ruled out as the *cause* of the loop:
        topic/type naming, session-key collision (distinct node names),
        `nros_cpp_spin_once` routing (already `executor.spin_once`), the
        backend poll ring (works for C), and `Executor::open`/spin/drive_io/
        try_recv re-opening (each opens at most once per boot).

        **Fault-handler experiment 2026-05-25 (temporary, reverted).** Added a
        `k_sys_fatal_error_handler` override to the listener that prints the
        reason and halts instead of rebooting, plus per-iteration loop markers.
        Result: the override **never fired** (FATAL count 0) yet the listener
        **still rebooted 3×** — so the restarts do **not** go through Zephyr's
        fatal path (not a `k_panic`/exception/`__ASSERT`). And in the
        longest-lived generation the loop ran cleanly to `iter=192`
        (spin_once + try_recv each iteration, no halt) while still receiving
        **0** messages, with the talker publishing 1..8 normally and booting
        once. So there are two intertwined problems:
        1. A **non-fatal listener restart** (sys_reboot-style or a native_sim
           process re-exec — not a Zephyr fatal), specific to the C++ XRCE
           listener.
        2. **C++ XRCE pubsub `try_recv` delivers 0 even within a single
           stable generation** that never restarts — so reception is broken
           independently of the restarts.
        Next step needs interactive debugging the sandbox can't provide: run
        the listener `native_sim` `.exe` under gdb outside the sandbox (the
        binary binds a UDP socket the sandbox SIGKILLs → exit 144) to catch
        the restart trigger, and trace the XRCE input-stream → `xrce_topic_callback`
        → ring → `try_recv` path within one stable generation to find why
        buffered samples aren't drained by `try_recv`.

        **Restart further characterized 2026-05-25 (more temporary printk,
        reverted).** Per-loop-position markers (`pre-spin`/`post-spin`/
        `post-recv` + iteration counter) prove:
        * It is a **genuine whole-image reboot** — the iteration counter
          resets `0…~190 → reboot → 0…~163 → reboot`, and `main` re-runs.
        * The reboot fires **inside `nros::spin_once`** (the last marker before
          a boot banner is always `<it> A pre-spin`, never `B post-spin`).
        * It is **not** data-correlated: the talker finishes publishing by
          ~10 s, but the reboots land at a roughly constant ~160–190 loop
          iterations (~18–20 s) into each generation.
        * Overriding `sys_reboot()` in the listener did **not** fire (0 prints)
          yet it still rebooted 3× → not the Zephyr `sys_reboot` API.
        * No `CONFIG_WATCHDOG` / `CONFIG_TASK_WDT` in the build (same as the
          C listener), so not a watchdog. The only build-config delta vs the
          working C listener is `CONFIG_CPP=y` + `CONFIG_MINIMAL_LIBCPP=y`
          (C++ runtime); heap/stack sizes are identical, and the C++ XRCE
          *service* uses the same runtime and does **not** reboot — so it is
          specific to the C++ XRCE *pubsub* spin path.
        Net: a non-fatal, non-`sys_reboot` whole-image re-init triggered from
        inside `spin_once` after ~190 iterations, only on the C++ XRCE pubsub
        listener. Remaining unknowns (need gdb / native_sim internals): the
        native_sim code path that re-inits the image, and what accumulates
        over ~190 spins (executor arena / heap / uClient stream resource) that
        trips it.

  **CycloneDDS slice — `native_sim` runtime: GREEN 2026-05-25.** An earlier
  write-up here claimed the runtime was blocked on a missing `k_thread`
  ddsrt Zephyr thread port (`tid ... is in use!` → "data plane dead"). That
  was **wrong** — it was diagnosed against *stale `eth_posix` fixtures* with
  no network. Corrected with fresh NSOS-overlay fixtures (rebuilt via
  `NROS_ZEPHYR_FIXTURE_FILTER='build-.*-cyclonedds' just zephyr
  build-fixtures`):
  - The Cyclone worker threads **are** `k_thread`s — `CONFIG_POSIX_THREADS=y`
    routes ddsrt's `pthread_create` to Zephyr's POSIX pthread, and
    `CONFIG_DYNAMIC_THREAD=y` gives them dynamic stacks. There is no
    host-pthread / no-Zephyr-port problem.
  - The `os: tid ... is in use!` lines are **benign**. They come from
    `kernel/dynamic.c:132` (`z_impl_k_thread_stack_free` refuses to free a
    dynamic stack whose thread is still alive, returns `-EBUSY`); the free is
    simply declined and the thread keeps running. The participant is created
    and the data plane runs normally despite the log noise.
  - Verified: a 2-node native_sim cpp talker↔listener exchanges data
    (`Received: 13`), and the full `dds` nextest group is **15/15 PASS**
    (`binary(zephyr) and test(dds)`, NSOS fixtures): all c/cpp talker /
    listener / service / action boots **plus** `*_action_e2e` for C, C++,
    and Rust.

  No ddsrt thread port is needed. Remaining Cyclone-on-Zephyr nits (e.g. the
  C++ action feedback drain characterized in the action-feedback follow-up)
  are tracked separately, not by this slice. The `tid ... is in use!` log
  spam could optionally be silenced by giving ddsrt threads a non-dynamic
  stack, but that is cosmetic.
  - [x] `test_zephyr_dds_{c,cpp}_{talker,listener,service_*,action_*}_boots`,
        `test_zephyr_dds_{c,cpp,rs}_action_e2e` — 15/15 pass on NSOS fixtures.

- [x] **177.9.G - NuttX action E2E runtime.**
  Closed 2026-05-25. Focused rerun passed after building the required
  NuttX fixtures with the repo SDK environment:
  `source scripts/sdk-env.sh; just nuttx build-fixtures`, then
  `cargo nextest run --cargo-profile nros-fast-release -p nros-tests
  --test rtos_e2e --no-fail-fast -E "binary(rtos_e2e) and
  test(test_rtos_action_e2e::platform_2_Platform__Nuttx) and
  (test(lang_2_Lang__C) or test(lang_3_Lang__Cpp))"`. The setup needed
  `build/zenohd/zenohd` and `rust-src` for the pinned NuttX nightly so
  the C++ generated FFI crates could use `-Z build-std`.
  - [x] `rtos_e2e::test_rtos_action_e2e::platform_2_Platform__Nuttx::lang_2_Lang__C`
  - [x] `rtos_e2e::test_rtos_action_e2e::platform_2_Platform__Nuttx::lang_3_Lang__Cpp`

- [x] **177.9.H - Flaky but recovered.**
  Closed 2026-05-25. Not reproducible under focused rerun: after staging
  the ThreadX-Linux zenoh C++ fixtures
  (`examples/threadx-linux/cpp/{talker,listener}/build-zenoh/`), the test
  passed 17/17 consecutive runs (16 retries-off + 1 verbose), the verbose
  run showing the talker publishing 0..9+ and `messages received: 11`.
  Command:
  `cargo nextest run -p nros-tests --retries 0 -E 'binary(rtos_e2e) and test(test_rtos_pubsub_e2e::platform_3_Platform__ThreadxLinux::lang_3_Lang__Cpp)'`.
  The lone 2026-05-22 failure was a host-load hiccup during the heavy
  parallel `test-all` sweep, not a product bug. The post-sweep readiness
  gate (`ensure_ready` waits for the listener's "Waiting for messages"
  marker before the talker window, `4b1b0723d`) plus the test design
  (talker publishes repeatedly across a 15 s window, listener collects for
  30 s and needs only one message) make the discovery race non-fatal.
  - [x] `rtos_e2e::test_rtos_pubsub_e2e::platform_3_Platform__ThreadxLinux::lang_3_Lang__Cpp`

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

### Code Review Findings (2026-05-25)

Post-merge review of the `db0e4fbb5` ThreadX Cyclone fix plus the build/test
re-org (`23c750514` just groups, `6fd5bd671`/`b38bcbadf` nextest profiles,
`6644372dd` focused native lanes). Functional today; items below are
robustness/consistency follow-ups, not regressions.

- [x] **177.23.A - `sertype_min.cpp` ThreadX guard fails open.**
  Fixed 2026-05-25. The CDR `opt_size_xcdr1/2` disable was gated on
  `#if DDSRT_WITH_THREADX` (`packages/dds/nros-rmw-cyclonedds/src/sertype_min.cpp`).
  That macro is Cyclone-internal — defined in the generated `dds/config.h`
  from `set(DDSRT_WITH_THREADX ${WITH_THREADX})` — and reached the TU only by
  transitive include, so if `config.h` ever left the include chain the `#if`
  evaluated 0, the optimization re-enabled, and the ThreadX ops-walker trap
  returned with **no compile error**. Now guarded on `NROS_PLATFORM_THREADX`,
  set explicitly `PRIVATE` on the target (`CMakeLists.txt:98`), matching the
  sibling `session.cpp`. The `#else` branch (non-ThreadX) is unchanged, so
  native/POSIX builds are unaffected.

- [x] **177.23.B - Two divergent fast-path test filters.**
  Fixed 2026-05-25. `just native test` (`just/native.just`) replaced the
  growing `not binary(...)` chain for the ROS 2 / XRCE-interop binaries with
  the `group(=ros2-interop)` + `group(=xrce_ros2_interop)` exclusion (same
  set as the old `params`/`rmw_interop`/`ros2_lifecycle_interop`/
  `xrce_ros2_interop` list, now drift-proof). The remaining explicit
  `binary(...)` excludes (`zephyr`, `esp32_emulator`, `large_msg`,
  `native_api`, `cpp_parameters`, `c_xrce_api`) are deliberate carve-outs
  with their own focused lane and no shared group. Unlike root `just test`,
  this lane intentionally keeps the QEMU RTOS e2e groups in, so it is not a
  verbatim copy of the root exclusion.

- [x] **177.23.C - just `[group(...)]` pass incomplete.**
  Fixed 2026-05-25. Added `[group(...)]` to the lifecycle recipes (setup/
  doctor → setup; build/test/clean/run → main; build-all/ci/*-fixtures/
  test-all → full-matrix; focused tests/probes → debug; kani/verus →
  verification) in the nine ungrouped module files (`just/workspace.just`,
  `verification.just`, `xrce.just`, `cyclonedds.just`, `rmw_zenoh.just`,
  `zenohd.just`, `docker.just`, `orin-spe.just`, `platformio.just`) — 65
  attrs total, matching the `freertos.just` convention (`default` + ad-hoc
  launchers stay ungrouped). **Root-recipe correction:** the root recipes
  flagged earlier (`build-test-fixtures-leaves`, `cyclonedds-ci`,
  `rust-rtos-link-check`, `check-*-mirror`, `check-example-matrix`,
  `format-{c,cpp,python}`, `check-{c,cpp,python}`, `build-workspace*`) are
  all `[private]`, so `just --list` already hides them — no grouping needed.

- [x] **177.23.D - "profile" name collides three concepts.**
  Fixed 2026-05-25. Review found **three** distinct "profile" concepts, not
  two: (1) the cargo build profile (`nros_nextest_profile_args` in
  `scripts/build/cargo.sh`, the `nros-fast-release` arg emitter — the worst
  offender), (2) the nextest run-profile `-P` (`nros_nextest_run_profile_*`),
  and (3) the recording overlay (`nros_nextest_profile_*`, `NROS_NEXTEST_PROFILE`,
  `.config/nextest-profile.toml`). Renamed (1) → `nros_cargo_nextest_args`
  (+ local var `cargo_nextest_args`) and (3) → `nros_nextest_record_*` /
  `NROS_NEXTEST_RECORD*` / `.config/nextest-record.toml`; (2) kept (already
  `run_`-prefixed). The recording path still leans on experimental nextest
  APIs (`store export`) — pin the nextest version when that surface
  stabilizes. Recording stays gated behind `NROS_NEXTEST_RECORD=1`, so
  normal runs are unaffected.

- [x] **177.23.E - Duplicate `177.22` item number.**
  Fixed 2026-05-25. Three items shared `177.22`: "ThreadX Cyclone
  participant init runtime trap" (kept — matches commit `db0e4fbb5` and the
  CLAUDE.md reference), "Zephyr CycloneDDS fixtures fail after Cyclone setup"
  (renumbered 177.24), and "Make `nros` the canonical build/test CLI"
  (renumbered 177.25).

## Closed

- [x] **177.25 - Make `nros` the canonical build/test CLI.**
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
- [~] 2026-05-25 full nuke gate (`just clean` → `just setup all` →
  `build-all` → `build-test-fixtures` → `test-all`). `clean`/`setup`/
  `build-all` (4817s)/`build-fix` (1710s) all OK; `test-all` (755s) ran
  978 tests with **13 real failures** + 15 `[SKIPPED]` (env / opt-in
  fixtures). None touch the Zephyr XRCE/cpp work closed under 177.9.F.
  The 13 cluster as: ROS 2 detection ×2 (`ros2::test_ros2_detection`,
  `test_rmw_fastrtps_detection` — no sourced ROS 2 in the run env),
  `integration_esp_idf` smoke ×1 (idf.py env not active), `rtos_e2e`
  Rust+Cpp ×7 (Nuttx Rust pubsub/service/action + Nuttx Cpp action +
  ThreadxLinux Rust pubsub/service/action — Rust-host variants, not the
  C/Cpp paths closed in 177.9.G/H), `logging_smoke` ×2 (esp32_qemu,
  zephyr_native_sim), `zpico_drift_gate_fires_on_corrupted_include` ×1.
  Pre-existing, untriaged.
- Two build-all-after-clean fragilities surfaced by the nuke gate:
  - **(fixed, `6e1d26dee`)** jobserver prefetch ran `cargo fetch
    --locked` on standalone example/fixture dirs whose gitignored
    `Cargo.lock` goes stale after a clean+setup → hard-fail. Dropped
    `--locked` in `scripts/build/cargo.sh::nros_cargo_fetch_standalone_manifests`.
  - **(open, worked around)** `build-all` jobserver prefetch assumes
    example `generated/` dirs exist, but they are gitignored and a clean
    wipes them; esp32 prefetch failed until I pre-ran `nros generate-rust`
    across the 74 Rust examples. The codegen step must run (or be
    tolerated) before the prefetch fanout — systemic ordering bug, not
    yet fixed. Candidate 177 follow-up.

## Archive Rule

Archive this tracker only after:

- [x] 177.3 closes or moves into a newer, more specific phase doc.
- [ ] 177.6 through 177.9 have owners and either close or move into more
  specific phase docs.
- [x] 177.19 and 177.20 close or move into platform-specific runtime
  phases.
