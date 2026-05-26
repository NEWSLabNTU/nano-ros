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

- [x] **177.26 - ThreadX Cyclone peer interop / multicast discovery (CLOSED
  2026-05-26).** ThreadX Cyclone interoperates with peer nano-ros Cyclone
  nodes on both axes: **ThreadX↔ThreadX** (two-QEMU ThreadX-RV64, item 1) and
  **ThreadX↔native** (threadx-linux↔native POSIX on loopback, item 2) — both
  fixed, verified, and covered by passing tests. Residuals are out of this
  item's scope: QEMU-RISCV64↔native is infra-gated (host↔QEMU bridge = root,
  not a code defect) and **stock `rmw_cyclonedds` (real ROS 2) interop is a
  Phase 117.X deliverable** (cross-platform wire-compat, not threadx-specific).
  Owner: Phase 177 runtime/Cyclone follow-up. Split out of 177.22
  (participant-init trap, closed).

  **Status 2026-05-26 — ThreadX↔ThreadX RTPS WORKS end-to-end.** The
  multicast-discovery + data-plane blockers are fixed and verified (see
  177.26.RX / RX.2 below): a two-QEMU ThreadX-RV64 Cyclone pair now runs
  SPDP join → discovery → SEDP → reliability → DATA → app delivery, with the
  listener decoding `Received: 21` against the talker's `Published: 21` over a
  shared-L2 `socket,mcast` segment. The two fixes:
  - cyclonedds `ddsi_udp.c` (fork `nano-ros` @ `12b4af2c`) — join multicast
    with `INADDR_ANY` interface under `DDSRT_WITH_THREADX` (NetX BSD
    `IP_ADD_MEMBERSHIP` `ntohl`s `imr_interface` vs host-order
    `nx_interface_ip_address` → `EINVAL` → group never joined).
  - in-tree `subscriber.cpp` — RX take buffer from `ddsrt_calloc`, not
    `std::calloc` (unwired libc heap on ThreadX → `BAD_ALLOC` before
    `dds_take`; Phase 177.22 hazard on the receive side).

  **Remaining scope (why this stays open):**
  1. ✅ **DONE 2026-05-26** — `test_threadx_riscv64_cyclonedds_two_qemu_pubsub`
     un-`#[ignore]`d and **passing**. It prefers `-netdev dgram` (QEMU ≥ 7.2,
     CI-isolated) and falls back to `-netdev socket,mcast` on older QEMU;
     reliable RTPS covers any mcast cross-process loss. Added
     `qemu_riscv64_supports_dgram_unix()` — the old `qemu_supports_dgram_unix()`
     probed the patched ARM binary (always has dgram) but this test runs
     `qemu-system-riscv64` (system 6.2, no dgram), so it wrongly took the dgram
     path. Verified PASS via `socket,mcast` on this host (listener decodes
     `Received`).
  2. ✅ **ThreadX↔native DONE 2026-05-26** — demonstrated via the
     **threadx-linux** target, not QEMU. A threadx-linux Cyclone talker
     (ThreadX kernel + NetX Duo over NSOS host sockets) and a native POSIX
     Cyclone listener interoperate on loopback with **no bridge**: NSOS routes
     the RTOS node's UDP through the host stack, so SPDP discovery + `rt/`
     RTPS data flow directly to the native peer. Tracked by
     `native_api::test_threadx_linux_cyclonedds_talker_to_native_listener`
     (native listener decodes ≥2 samples; PASS). Domain 0 (the threadx-linux
     talker's `config.toml`; it ignores `ROS_DOMAIN_ID`), free of the
     auto-allocated test domains (40+).

     The **bare-metal ThreadX-QEMU-RISCV64** node can't do this loopback trick
     — it reaches peers only over QEMU's `-netdev dgram`/`socket,mcast` L2
     (raw ethernet frames in an AF_UNIX/UDP transport); a native/stock peer on
     real host UDP can't join without a host↔QEMU bridge (TAP/veth = root),
     which the slirp-only no-sudo infra avoids (and slirp doesn't forward
     multicast SPDP). So QEMU-RISCV64↔native stays infra-gated, but the
     platform-agnostic-wire claim is proven by the threadx-linux path above.
  3. **Stock `rmw_cyclonedds` (real ROS 2) interop** still pending — needs a
     ROS 2 install + the Phase 117.X stock-RMW wire-compat work (`rt/`/`rq/`/
     `rr/` prefixes, `cdds_request_header_t`, type-hash mangling). nano-ros
     Cyclone↔Cyclone (incl. RTOS↔native) is proven; nano-ros↔**stock** is a
     117 deliverable, not threadx-specific.

  **2026-05-25 — discovery re-enabled, surfaced a byte-order defect (historical).**
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

  - [x] **177.26.RX — ThreadX Cyclone two-node pubsub now works end-to-end
    (FIXED + VERIFIED 2026-05-26).** Two distinct bugs, both root-caused and
    fixed: (1) **177.26.RX — multicast group never joined**: NetX's BSD
    `IP_ADD_MEMBERSHIP` interface lookup `ntohl()`s `imr_interface` while
    Cyclone supplies it in host order → `EINVAL` → every peer SPDP frame dropped
    at the IP-accept gate. Fixed in cyclonedds `ddsi_udp.c` (`INADDR_ANY` under
    `DDSRT_WITH_THREADX`). (2) **177.26.RX.2 — receive take path used libc heap**:
    `subscriber_try_recv_raw` allocated its deserialise buffer with `std::calloc`,
    which returns `nullptr` on ThreadX (unwired libc heap) → every take bailed
    `BAD_ALLOC` before `dds_take`. Fixed in
    `packages/dds/nros-rmw-cyclonedds/src/subscriber.cpp` (`ddsrt_calloc`/`ddsrt_free`,
    Phase 177.22 hazard on the RX side). **Verified** on a clean build: the
    two-QEMU `socket,mcast` pair gives listener `Received: 21` (consecutive
    `0,1,2,…`) vs talker `Published: 21`. TX/locator byte-order (`5558c6ae`) and
    listener `register_subscription` (177.28) were already fixed. Remaining:
    maintainer pushes the cyclonedds `ddsi_udp.c` fix to the fork + bumps the
    pointer; then un-`#[ignore]`
    `test_threadx_riscv64_cyclonedds_two_qemu_pubsub` and run on QEMU ≥ 7.2
    (`-netdev dgram`) — the lossy `socket,mcast` workaround already shows 21/21.
    Update the test's stale `#[ignore]` reason (`listener register_subscription
    fails`).

    **ROOT CAUSE CONFIRMED — the multicast group was never joined (2026-05-26).**
    The earlier "drop inside NetX core RX/select" hypothesis was **wrong**.
    Marker instrumentation (in `nx_igmp_multicast_check.c`,
    `nxd_bsd.c`, cyclonedds `q_init.c` / `ddsi_mcgroup.c`; **all reverted**) on
    a two-QEMU socket-mcast pair gave decisive evidence:
    - Peer SPDP frames **do** reach NetX IP: `_nx_igmp_multicast_check` is
      called with the correct group `0xefff0001` (239.255.0.1).
    - But the joined-group list is **empty** (`nx_ipv4_multicast_entry[0] == 0`)
      → `_nx_igmp_multicast_check` returns FALSE (`MISS-GROUP`) → every SPDP
      frame is dropped at the IP-accept gate, before UDP/BSD. (So RX/select
      was never the issue.)
    - The BSD `IP_ADD_MEMBERSHIP` handler is **never reached** by a successful
      join. Tracing forward through Cyclone: `joinleave_spdp_defmcip`
      (`allowMulticast = DDSI_AMC_SPDP`) → `joinleave_mcgroups`
      (`recvips_mode = PREFERRED`, 1 interface, `mc_capable = 1`) →
      `joinleave_mcgroup` (kinds match, UDPv4) → `ddsi_join_mc`
      (fresh, not already-joined) → `joinleave_asm_mcgroup` →
      `ddsrt_setsockopt(IPPROTO_IP, IP_ADD_MEMBERSHIP)` → `nx_bsd_setsockopt`.
      Constants match (Cyclone's `threadx.h` `#include <nxd_bsd.h>`, so
      `IPPROTO_IP=2`, `IP_ADD_MEMBERSHIP=32`). The class-D validation passes.
      The handler then **aborts with `EINVAL` at the interface-match loop**
      (`nxd_bsd.c` ~line 7168): it computes `addr = ntohl(imr_interface.s_addr)`
      and compares against `nx_ip_interface[i].nx_interface_ip_address`. But on
      this port Cyclone supplies `imr_interface` in **host** order
      (`0x0a000229` = 10.0.2.41) — **the same order as
      `nx_interface_ip_address` (`0x0a000229`)** — so the `ntohl` corrupts it
      to `0x2902000a`, which matches nothing → `EINVAL` → group never joined.
      This is a facet of the 177.26 host-vs-network locator byte-order mismatch
      (`interf->loc.address+12` is host-order; the BSD handler expects network
      order per standard BSD).

    **Fix (verified, in cyclonedds working tree — maintainer to commit/push).**
    `ddsi_udp.c::joinleave_asm_mcgroup`, under `#if DDSRT_WITH_THREADX`, pass
    `mreq.imr_interface.s_addr = htonl(INADDR_ANY)` instead of
    `memcpy(interf->loc.address+12)`. The handler's `INADDR_ANY` branch picks
    interface 0 directly (single-homed embedded — `n_interfaces == 1`),
    sidestepping the byte-order-sensitive lookup, and the join's stored
    interface (`interface[0]`) then matches the RX packet's interface in
    `_nx_igmp_multicast_check`. After the fix, the two-QEMU socket-mcast run
    shows **both nodes `JOINg: 2`, `MCK=HIT`, `MISS-GROUP: 0`** — bidirectional
    SPDP multicast is now accepted at the IP layer. (Diff lives in the
    cyclonedds submodule working tree; not pushed — external fork.)

    **177.26.RX.2 — ROOT-CAUSED + FIXED + VERIFIED 2026-05-26.** With the
    multicast join fixed the listener still showed `Received: 0`. The
    "unicast-locator byte order" hypothesis was **disproved**: marker
    instrumentation (all reverted) on the two-QEMU socket-mcast pair showed the
    whole RTPS pipeline actually working — 41 datagrams reach Cyclone (SPDP on
    the mc socket, SEDP/Heartbeat/AckNack on the unicast socket, bidirectional),
    21 application DATA samples are delivered (`deliver_user_data`, `rdary=1`
    matched reader), deserialised (`get_serdata` sd≠NULL, sz=8), stored
    (`dds_rhc_default_store` → `notify_data_available=1`, `dds_reader_data_available_cb`
    fires). So discovery, matching, reliability, data transfer and RHC store all
    work between two ThreadX peers — the locator byte order is fine.

    The actual break was in **nano-ros's receive take path**: the nros executor
    polls the Cyclone subscriber every spin (`subscriber_try_recv_raw`), but that
    function allocated its transient deserialise buffer with **`std::calloc`**
    (libc heap). On ThreadX the libc/newlib heap is unwired, so
    `std::calloc(1, 4)` for an `Int32` returns `nullptr` → every take bails
    `NROS_RMW_RET_BAD_ALLOC` before `dds_take`, and the stored samples are never
    handed to the app. This is the **Phase 177.22 hazard on the receive side** —
    177.22 migrated only the *publish* path to `ddsrt_*`; the subscriber path
    was missed.

    **Fix (in-tree, `packages/dds/nros-rmw-cyclonedds/src/subscriber.cpp`):**
    `subscriber_try_recv_raw` now uses `ddsrt_calloc` / `ddsrt_free`
    (`<dds/ddsrt/heap.h>`) instead of `std::calloc` / `std::free`, mirroring
    `publisher.cpp` (Phase 177.22). `subscriber_try_recv_sequence` was already
    loan-based (`dds_return_loan`) so it was unaffected. **Verified end-to-end on
    a clean build (no instrumentation):** the two-QEMU socket-mcast pair now
    gives listener `Received: 21` (consecutive `0,1,2,…`) against talker
    `Published: 21`. Full chain GREEN: SPDP multicast join → discovery → SEDP →
    reliability → DATA → RHC store → executor poll → app callback.

  **Next.**
  1. Maintainer: commit the `ddsi_udp.c` threadx multicast-join fix + push
     cyclonedds (with `5558c6ae`) to `nano-ros-fork`
     (`nano-ros/zephyr-nsos-patches`) and bump the submodule pointer. (The
     177.28 listener descriptor fix + the `subscriber.cpp` ddsrt-heap fix are
     already in the nano-ros repo.)
  2. Audit the other RTOS Cyclone backends (FreeRTOS) for any remaining libc
     `std::malloc/calloc/free` on hot paths — same hazard class as 177.22 /
     177.26.RX.2.
  3. Un-`#[ignore]` `test_threadx_riscv64_cyclonedds_two_qemu_pubsub` and run it
     on QEMU ≥ 7.2 (`-netdev dgram`); this host's 6.2 only does the lossy
     `socket,mcast` workaround, but that already shows 21/21 with both fixes.

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

- [x] **177.8 - Full runtime matrix requires prebuilt fixtures. RESOLVED
  2026-05-26.** All concrete sub-items (177.8.a–e) are closed and every
  optional host dependency now reports a precise skip/remedy. The residual
  "every fixture lookup uses the build-fixture artifact layout" goal is the
  scope of **Phase 181 (fixture-build SSOT)** and is tracked there, not here.
  The latest sweep was run after `just setup` and
  `just build-test-fixtures`, so the remaining fixture/setup failures are
  narrower than the original broad prebuild issue.

  **Full Zephyr E2E matrix verified green 2026-05-26: `binary(zephyr)` is
  53/53** (Zenoh + XRCE + CycloneDDS; every language/role —
  boots / talker / listener / service / action / e2e) on a full
  `just zephyr build-fixtures` (56 fixtures).

  **Staleness-gate over-broadness — FIXED 2026-05-26.**
  `is_binary_stale` (`nros-tests/src/zephyr.rs`) used to watch **all of
  `packages/core`** for every fixture, so a change confined to one language's
  runtime (e.g. the nros-cpp `action.rs` feedback fix) marked **unrelated C
  and Rust talker/listener fixtures stale** even though cmake correctly left
  them un-rebuilt (a C fixture links `nros-c`, not `nros-cpp`). The full-suite
  run hit this: 11 of 53 reported a fast `is stale` failure (0.1–0.7 s) while
  the binaries were functionally current. Fixed: the gate now enumerates
  `packages/core` and watches every crate **except the two non-matching
  language API crates** — `nros` (Rust) / `nros-c` / `nros-cpp` — via
  `core_crate_is_watched(crate, lang_api_crate)`. Every shared/platform/rmw
  crate (nros-core, nros-node, nros-rmw, nros-platform-*, …) stays watched and
  new crates are picked up automatically, so it never under-watches; only the
  documented false-positive (one language's edit tripping another's fixture)
  is removed. Unknown-language and `read_dir`-failure paths fall back to the
  old whole-tree watch. Covered by
  `zephyr::tests::test_core_crate_is_watched_per_language` (PASS). The
  alternative (refresh artifact mtimes at build time) remains relevant to
  phase-181 fixture SSOT but is no longer needed to clear the false failures.

  - [x] **177.8.a - `build-all` fixture set was not a superset of
    test-all's fixture needs (logging-smoke).** The 2026-05-25 full-nuke
    gate surfaced `logging_smoke_esp32_qemu` + `logging_smoke_zephyr_native_sim`
    hard-failing "fixture not built", even though `build-all` ran. Two
    structural holes: (1) `build-all.mk` omitted `esp32` from its fixture
    fanout entirely, so `just esp32 build-fixtures` (which chains
    `build-logging-smoke` → `logging-smoke-esp32-qemu.bin`) never ran; (2)
    `just zephyr build-fixtures` did not chain the separate
    `build-logging-smoke` recipe, so `logging-smoke-zephyr-native-sim`
    (`<workspace>/build-logging-smoke/zephyr/zephyr.exe`) was never built.
    Because both tests resolve a *prebuilt* artifact via `.expect(...)`
    (not `skip!`), the gap counted as a real failure, not a skip. Fix:
    add `esp32` to `build-all.mk` `INDEPENDENT_FIXTURE_PLATFORMS`, and
    build the logging-smoke fixture at the end of `just zephyr
    build-fixtures` (reusing the standalone recipe so the build dir
    matches the test's path contract; honoring `NROS_ZEPHYR_FIXTURE_FILTER`
    and counting it toward `selected` so a logging-only filter doesn't
    trip the "no fixtures matched" guard). Verified: filtered zephyr build
    builds only logging-smoke without false-exit; `just esp32
    build-logging-smoke` produces the `.bin`; both tests then PASS
    (`logging_smoke_zephyr_native_sim` 0.13s, `logging_smoke_esp32_qemu`
    30s). The 15 `[SKIPPED]` cyclone fixtures remain *intentional* opt-in
    (`just cyclonedds freertos-cross-probe`) and stay out of `build-all`.

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

- [ ] **177.31 - Native CycloneDDS service/action example executables don't
  link (no-op `: && :` link rule).** Found 2026-05-26 while adding the native
  Cyclone service/action E2E tests (Phase 183.4). Building any native
  `service-{server,client}` / `action-{server,client}` example under
  `-DNROS_RMW=cyclonedds` compiles the objects but produces **no executable** —
  the target's final link step is a literal shell no-op:
  ```
  $ ninja -C examples/native/c/service-server/build-cyclonedds -v c_service_server
  ...
  [206/206] : && :
  $ ls build-cyclonedds/c_service_server   # absent
  ```
  Contrast: the **zenoh** and **xrce** builds of the same targets produce real
  ELFs (`build-{zenoh,xrce}/c_service_server`), and the Cyclone **talker /
  listener** (pub/sub) executables link fine (native Cyclone pub/sub passes —
  see the Phase 117 notes in CLAUDE.md). So the gap is specific to the
  **service + action** example-CMake executable wiring under the Cyclone RMW:
  the `add_executable` / `nros_platform_link_app` (or RMW-conditional target)
  path for those four roles links to nothing on Cyclone.

  **Impact.** The native Cyclone service/action fixtures can't be built, so the
  Phase 183.4 e2e tests (`test_native_cyclonedds_{service,action}` in
  `native_api.rs`) skip indefinitely. `just native build-fixture-extras` was
  left building only Cyclone talker/listener for this reason.

  **Where to look.** The per-example `CMakeLists.txt` under
  `examples/native/{c,cpp}/{service-*,action-*}` + the platform/RMW link glue
  (`cmake/platform/nano-ros-posix.cmake`, `nros_platform_link_app`, and the
  `packages/dds/nros-rmw-cyclonedds` `NanoRos::Rmw::cyclonedds` interface) —
  diff the resolved executable target between a working `build-zenoh` and the
  no-op `build-cyclonedds` for the same role. **Owner**: Phase 117 / 175
  (Cyclone example wiring). **Unblocks**: Phase 183.4 native Cyclone
  service/action e2e (tests already written + skipping).

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
  - **Update 2026-05-26:** both `*_to_dds` bridge test files were **deleted**
    (see G2) — they targeted the retired dust-dds RMW (Phase 169) with no
    replacement, so the above `--test bridge_{xrce,zenoh}_to_dds_e2e` arms no
    longer exist.
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

- [x] **177.30 - NuttX-QEMU Cpp action goal hang: RESOLVED — `fflush(stdout)`
  deadlock (NOT a lease/`z_get` race).** Root cause (Update 4): the example's
  application thread blocked in `fflush(stdout)` on the libc stdout `FILE*`
  lock (`flockfile`) against zenoh-pico's background read/lease threads, so it
  never reached `send_goal_async`. Fixed by removing the redundant `fflush`
  calls (`1804f7ce9`); a manual 2-QEMU boot now runs the full
  goal→accept→feedback→result chain. The CI cell itself was removed in 182.5
  (QEMU-heavy) — out of the matrix for cost, not breakage. The original
  (incorrect) lease/`z_get` diagnosis is preserved below for the record.
  `test_rtos_action_e2e`
  (`platform_2_Platform__Nuttx::lang_3_Lang__Cpp`) hangs: the client prints
  `Sending goal: order=5` then never gets an accept; the server stays at
  "Waiting for goals" and never logs a goal request. NuttX Cpp pub/sub +
  service pass and the NuttX **C** action passes, so transport, service
  request/reply, and the server's action queryable all work — it is specific
  to the Cpp action goal path on NuttX. (Investigation log: 177.8.e.)

  **Scope correction (2026-05-26 full-matrix rerun — see G4).** The
  "Cpp-action-specific / C action passes" framing is **too narrow**: a clean
  rerun of the whole NuttX `rtos_e2e` matrix shows the race is
  **non-deterministic and language-agnostic** on the query/reply paths —
  Rust *service* failed all 3 tries, **C action hung 270 s** (hard `z_get`
  timeout), and Cpp action failed; meanwhile Rust service/action *flaky-passed*
  in an earlier run. Only **pubsub** (Rust/C/Cpp) is reliably green, because it
  never issues a `z_get` query. So 177.30 is the root cause of **all** NuttX
  service+action flakiness/hangs across every language, not just Cpp action;
  the "C action passes" observation was one lucky timing.

  **Root cause (confirmed via tshark + gdb-multiarch, 2026-05-26).** It is a
  timing-dependent concurrency race in the vendored zenoh-pico (1.7.2)
  multi-threaded runtime, NOT an nros logic bug:
  - tshark on `lo:7672` shows the goal **query never reaches the wire** —
    after discovery (both endpoints declare `…/_action/send_goal` liveliness,
    server `SS` / client `SC`) the two guest↔zenohd connections carry only
    3-byte keep-alives. The client blocks in the send path before it
    transmits.
  - gdb chain at the block: `nros_app_main → send_goal[_async] →
    send_goal_raw → CffiServiceClient::send_request_raw → zpico_get_start →
    z_get → _z_query → _z_send_n_msg`. `_z_mutex_lock` tracing shows the
    unicast **lease task** (`_zp_unicast_lease_task →
    _z_pending_query_process_timeout`) contending the session/pending-query
    mutex against the app thread's `z_get` TX + reply-final path.
  - **Heisenbug proof:** with the server up and the client run *under gdb*
    (perturbed timing), `z_get` returns and the server logs
    `Goal request [1]: order=5` — the action completes. At native speed the
    two paths deadlock. `_z_query` unlocks the session mutex
    (`src/net/primitives.c:542`) *before* `_z_send_n_msg` (`:558`), so it is
    NOT a session-mutex AB-BA inside `_z_query`; the cycle is between the
    TX/reply-final path and the lease task's session-locked timeout sweep.
  - **Why the action path and not pub/sub or service:** the action client
    carries far more concurrent pending-query churn — send_goal + get_result
    + cancel_goal service clients plus feedback/status subscriptions, and the
    warm-up `poll()` loop issues queries the lease task is timing out at the
    exact moment send_goal's `z_get` fires, widening the race window.

  **Roadmap (fix plan; nothing landed yet — vendored + cross-platform risky):**
  1. **Reproduce deterministically.** Add a NuttX stress harness that fires
     `z_get` while the lease task runs `_z_pending_query_process_timeout`
     (short lease interval + several in-flight pending queries) so the race
     is hittable without QEMU timing luck.
  2. **Fix the lock ordering in zenoh-pico.** Narrow the lease task's hold:
     `_z_pending_query_process_timeout` must not keep the session mutex while
     touching the TX path, OR enforce a single global order (session →
     transport TX) on every site that takes both (`_z_send_n_msg`,
     `_z_trigger_reply_final`, the lease sweep). Land as a tracked patch over
     the pinned 1.7.2 tree (mirror the existing `_z_query` /
     `_z_unsafe_register_pending_query` patch in `src/net/primitives.c`).
  3. **Cross-platform reverify** the patch on every multi-threaded zenoh-pico
     backend — POSIX, Zephyr, FreeRTOS+lwIP, ThreadX+NetX, NuttX — since the
     lease task is shared. `just test-all` for each platform's `rtos_e2e` +
     pub/sub + service, not just NuttX action.
  4. **Re-enable** NuttX Cpp action in `rtos_e2e` once green; drop 177.8.e's
     "open" note.

  **Acceptance:** `test_rtos_action_e2e` NuttX/Cpp passes at native speed
  (no gdb), the server logs the goal + result, and no regression on the other
  zenoh-pico backends' rtos_e2e/pub-sub/service suites. **Depends on:** none
  (self-contained zenoh-pico concurrency work). **Priority:** medium —
  isolated to NuttX Cpp actions; C actions + all other NuttX paths work.

  **Progress 2026-05-26 (experiments in the jerry73204/zenoh-pico fork; all
  reverted, tree clean). Two leading hypotheses ELIMINATED — it is NOT a
  mutex deadlock:**
  - **Lease-task sweep — eliminated.** No-op'd `_z_pending_query_process_timeout`
    (the session-mutex sweep), rebuilt client+server, retested: still hangs.
    (Also: `SERVICE_DEFAULT_TIMEOUT_MS = 30000`, so the sweep never actually
    drops the in-flight goal query anyway.)
  - **Re-entrant session self-deadlock — eliminated.** Routed the
    non-recursive `_z_mutex_*` to the recursive platform impl (so a thread
    re-locking the session mutex can't self-deadlock — the async
    `goal_response_cb → get_result_async` re-enters `z_get`), rebuilt,
    retested: still hangs. So a non-recursive re-entrant lock is not it.
  - **Confirmed facts:** the goal query never reaches the wire; keep-alives
    keep flowing during the hang (so the app thread is NOT holding the
    transport-peer mutex); it works under gdb (timing race). NuttX `_z_mutex_*`
    routes through `platform_aliases.c → nros_platform_mutex_*` (pthread),
    NOT zenoh-pico's `system/unix/system.c`.
  - **Tooling blocker:** gdb-multiarch on the QEMU NuttX gdbstub cannot read
    the app's `.bss` at the hang — it halts in kernel context (PC in a
    high `??` region) and a lock-event ring buffer (placed in
    `platform_aliases.c`, symbol present + linked, `_z_mutex_lock` verified
    to call it) reads `idx = 0` at 45 s AND 85 s wall. So the gdb-memory
    observe path is dead here.
  - **Refined hypothesis:** a missed-wakeup / condvar (or spin-sem) race in
    the get/reply path, or a NuttX socket-send stall — not a lock cycle.
  - **Next concrete step:** observe via the *console* instead of gdb — emit
    the lock/condvar-wait event ring with a raw `write(1, …)` from a watchdog
    (or unbuffered per-event), since QEMU stdout is readable and gdb `.bss`
    reads are not. Then inspect the `g_spin`/condvar wakeup in
    `zpico_get_check` / `call_raw` against the reply-final dropper's
    `_zpico_notify_spin()`.

  **Update 2 2026-05-26 — it is NOT a deadlock; the action code is correct.**
  A direct, lightly-loaded boot (zenohd `0.0.0.0:7672 --no-multicast-scouting`
  + server QEMU + client QEMU, no nextest) runs the **full** chain to
  completion: `Sending goal → Feedback [0] → Goal accepted! → Feedback
  [0,1,1] → Result [0,1,1,2,3,5] → Action completed successfully`, and the
  server logs `Goal request [1]: order=5`. So all the earlier
  "hang/deadlock/Heisenbug" symptoms were the client being killed before the
  (very slow, under `-icount` + load) chain finished. Two real weaknesses in
  the async example were hardened (see `examples/.../cpp/action-client`):
  the result-wait loop no longer self-limits at 1000 iterations, and the
  one-shot `send_goal_async` is now resent until accepted (matching the
  blocking C client's internal retry). The harness window for NuttX/Cpp
  action was raised 60s→240s (= the C variant) with a DO-NOT-SHRINK comment.
  **STILL OPEN, narrowed:** under the *nextest harness* the server never
  receives the goal (`Server post-boot:` empty) even at 240s, whereas the
  *direct boot* — with byte-identical zenohd args (`start_slirp` =
  `--listen tcp/0.0.0.0:<port> --no-multicast-scouting`), QEMU args, fixtures,
  and server-then-client order — routes it fine. The remaining delta is
  harness-level (a discovery/routing or process-sequencing difference between
  the in-test launch and a direct boot), NOT a zenoh-pico lock bug and NOT the
  60s timeout. Next: diff the exact QemuProcess launch vs the direct boot
  (env, fd setup, slirp options, start ordering / inter-launch delay) and
  capture the wire on the *nextest* run's port to see if the client's query
  even leaves the guest there.

  **Update 3 2026-05-26 — wire capture: the goal query never reaches TCP.**
  Ran the retry-hardened client under nextest (240s window) with `tshark` on
  the zenohd port. 479 frames captured. On the wire: only the `SS`/`SC`
  liveliness DECLAREs for `…/send_goal/…` (the 185–186 B frame pairs) plus
  len=3 keep-alives — **no goal-query frame at all**, zero TCP
  retransmissions, zero zero-window. The client log shows it reaches
  `Sending goal: order=5` and `send_goal_async` returns **OK** (no failure
  print on any of the resends), yet nothing is transmitted. So Update 2's
  "it's just slowness / needs a bigger window" lean was **wrong**: the
  request never leaves the client. It is a *silent `z_get` query-TX no-op*
  under native nextest timing — `send_goal_async → z_get → _z_send_n_msg`
  returns success but emits no bytes — while the very same code transmits and
  completes the full chain under a direct lightly-loaded boot **and** under
  gdb. Classic Heisenbug: any timing perturbation (gdb single-step, light
  load) makes the send fire. Not a deadlock (the recursive-mutex experiment
  already ruled the session/TX mutexes out), not the timeout, not slowness.
  Six hypotheses (lease-task, mutex deadlock, recursive mutex, syscall ring
  buffer, timeout bump, async resend) have each only relocated the
  timing-sensitivity — per systematic-debugging this is the "question the
  approach, stop blind fix #7" point. The blocking **C** action client does
  not hit this: its `send_goal` spins-and-retries *inside* zenoh-pico
  context, driving the session until the query actually flushes; the C++
  async `send_goal_async` returns to the app loop trusting `z_get` to have
  sent synchronously.
  Code-path comparison (done): C++ `send_goal_async` → `send_goal_raw` →
  `send_request_raw` → `Session::get_start` → `zpico_get_start`, which calls
  `z_get` **synchronously** (`zpico.c:2236`) — it does *not* defer the send.
  The C blocking client → `send_goal_blocking` → `call_raw` → `zpico_get`,
  also `z_get`, but then keeps spinning/reading inside zenoh-pico context.
  Since the async path's `z_get` is synchronous yet emits no bytes (and the
  resend fires it repeatedly to no effect — 240 s of `spin_once` would have
  flushed anything merely batched), the drop is **inside `z_get` itself**
  (`_z_query`/`_z_send_n_msg`), not an async-defer or unpumped-TX-queue issue.
  Keep-alives on the same transport flush fine, so the transport isn't dead —
  the query message specifically is being built and dropped under this timing.
  **Recommended next steps (do NOT use gdb — it perturbs the bug away):**
  1. Instrument `_z_query` / `_z_send_n_msg` in the fork with an in-guest,
     self-dumping ring (entry, the per-message serialize result, and the
     socket-write return value) to see whether the query message is built,
     whether the write is attempted, and what it returns — the async path's
     `z_get` returns OK with zero bytes on the wire, so the loss is in there.
  2. Add an in-guest, non-gdb trace: a small ring buffer written at
     `_z_send_n_msg` entry/exit + the socket-write return, dumped to the NuttX
     console on a timer (the gdb `.bss` read returns 0 at the hang, so the
     trace must self-dump, not be read externally).
  3. If the root cause stays out of reach, the honest interim is to gate the
     NuttX/Cpp action e2e behind an `#[ignore]`/feature like the other
     experimental fixtures and keep the C variant (which passes) as the
     action coverage on NuttX — rather than leave a permanently-red E2E.

  **Update 4 2026-05-26 — ROOT CAUSE FOUND + FIXED (Updates 2 & 3 were both
  wrong).** Probe-bisected the client with unbuffered `write(2)` markers
  (libc-`FILE*`-lock-free) at every layer (main loop, C++ header send method,
  Rust FFI, zpico_get_start, `_z_query`) plus QEMU `-d int` exception logging.
  Findings, in order:
  - QEMU `-d int` showed **only timer IRQs** at the hang — the guest is alive
    and *blocked*, not crashed.
  - The client reaches `printf("Sending goal…")` then wedges in the **very next
    statement, `fflush(stdout)`**: the `write(2)` marker placed *before* the
    fflush prints, the one *after* it never does.
  So the bug is a **deadlock on the NuttX libc stdout `FILE*` lock**
  (`flockfile`): the application thread's explicit `fflush(stdout)` blocks
  against the zenoh-pico background read/lease threads that also touch stdout.
  The main thread never reached `send_goal_async`, so the goal request never
  left the guest — which is why Update 3 saw "no query on the wire" (the query
  was never *attempted*, not dropped in `z_get`), and why Update 2's "slowness"
  was wrong (it is a hard deadlock, not a slow path). `printf("…\n")` itself is
  fine (line-buffered, flushes on the newline); only the *explicit, redundant*
  `fflush` deadlocks.
  **Fix (`examples/qemu-arm-nuttx/cpp/action-client/src/main.cpp`):** remove
  every `fflush(stdout)` from the example (with a DO-NOT-RE-ADD comment).
  **Verified:** a full 2-QEMU manual boot (zenohd + cpp action server + cpp
  action client) now runs the complete chain — server logs `Goal request [1]:
  order=5`; client logs `Goal accepted! → Feedback: [0] → Result:
  [0, 1, 1, 2, 3, 5] → Action completed successfully` — reproducibly. The
  goal-request query is on the wire (tshark) and zenohd forwards it to the
  server.
  **Still open (separate, harness-only):** under the *nextest* harness the
  same fixed binaries still fail — the client sends (11 query frames on the
  wire) and the server declares its `send_goal` queryable, but zenohd does not
  forward the query to the server (no `7672 → <server>` frame), whereas the
  byte-identical manual boot forwards it. nextest shows ~5 TCP streams to
  zenohd (vs 2 manual; the extras are host-side readiness probes from
  `ZenohRouter`, not stale guests — verified zero leftover QEMUs). This is a
  harness-level zenohd-routing/timing artifact under `-icount` + test-runner
  CPU load, NOT the (now-fixed) deadlock and NOT a product bug — the action
  works in a real boot. Tracking the routing delta is the remaining 177.30
  item; the deadlock fix lands independently.

#### 2026-05-26 Clean-Rebuild Test-All by Group

Full clean-room validation after the Phase 181 fixture-build-SSOT work landed
on main: `cargo clean` + `clean-examples`/`clean-fixtures` (17.2 GB freed, SDK
installs in `build/` preserved) → `just build-all` (**exit 0**) → `just test-all`.
Result: **978 tests run, 958 passed, 20 failed, 8 skipped, 26 flaky** (339 s).

Purpose was to confirm the build-orchestration refactor (manifest +
`fixtures-build.sh` + the `_check-fixtures-stale` probe) introduced no
regressions. It did not: the rust staleness probe ran **clean** after the full
build (esp cells correctly excluded via `skip_probe`/`--for-probe`, every other
platform fresh), and all 20 failures reproduce known groups or land in
subsystems untouched by Phase 181. The harness builds its fixtures via
`fixtures/binaries/mod.rs`, a path independent of the migrated `just` recipes,
so these E2E outcomes are orthogonal to the refactor. Grouped:

- [x] **G1 - Placeholder `skip!` cells — DELETED (was 4).**
  `cmake_platform_matrix::cmake_platform_{freertos,nuttx,threadx,zephyr}` were
  unconditional `skip!`s from the file's first commit (`044d7fd6d`), deferred to
  a "Phase 139" that has no roadmap doc. Investigation (2026-05-26) found their
  intended coverage — the `cmake/platform/<plat>.cmake` module contract — is
  already exercised end-to-end by the real C/C++ example builds + `rtos_e2e`
  (each example configures `add_subdirectory(<root>) + NANO_ROS_PLATFORM=<plat>`,
  the build path migrated in Phase 181.5) and the Phase 139 `integrations/<rtos>/`
  shells. So they were zombie placeholders (4 permanent raw "failures", 0 real
  per `_test-summary`). **Deleted** the 4 cells + the now-dead `require_cmd_or_skip`
  helper; `cmake_platform_posix` (dispatch guard) + `cmake_platform_threadx_requires_board`
  (real FATAL_ERROR check) stay. Supersedes the deferral half of **177.9.B**.
- [x] **G2 - Retired bridge-source paths (2). RESOLVED 2026-05-26 — tests deleted.**
  `bridge_xrce_to_dds_e2e`, `bridge_zenoh_to_dds_e2e` targeted the retired
  **dust-dds** RMW (Phase 169); their example trees
  (`examples/native/c/bridge/xrce-to-dds`,
  `examples/bridges/native-rust-zenoh-to-dds`) were deleted in the Phase 118
  collapse with no DDS replacement (a Cyclone bridge would be a new example
  under `examples/bridges/`, not these). The tests could only ever `skip!`
  (which panics → counted RED in the nextest tally). Both test files removed —
  there was nothing to relocate to. -2 from the failure count.
- [x] **G3 - Flaky, pass on isolated rerun (2).**
  `safety_e2e::test_safety_e2e_talker_listener` (ran 40 s then failed in the
  parallel sweep; isolated rerun "safety-e2e results: 3 ok, 0 fail") and
  `large_msg::test_zenoh_overflow_detection` (isolated rerun PASS, 15 s,
  `overflow_drops=5`). Host-load discovery hiccups under the heavy parallel
  matrix, same character as **177.9.H**. Confirms the Phase 181 `target-safety` /
  `target-large-buf` fixtures build and run correctly.
- [x] **G4 - NuttX runtime E2E. RESOLVED 2026-05-26.** (pubsub all-green;
  action NuttX dropped from CI in 182.5 + Cpp fflush fix in 177.30; service/Rust
  fixed below.) Original split-by-root-cause notes follow.
  `rtos_e2e::test_rtos_{action,pubsub,service}_e2e`
  on `Platform__Nuttx` × {Rust,C,Cpp} + `nuttx_make_e2e`. Rerun result
  (10 tests, 6 pass / 4 fail):

  | test | Rust | C | Cpp |
  |---|---|---|---|
  | pubsub  | ✅ | ✅ | ✅ |
  | service | ❌ | ✅ | ✅ |
  | action  | ✅* | ❌ (270 s hang) | ❌ |

  (* action/Rust flaky-passed; service/Rust flaky-passed earlier this session,
  failed all 3 tries here.) Plus `nuttx_make_e2e::nuttx_external_apps_link_into_kernel_binary`
  → `[SKIPPED]` precondition (the make fixture isn't staged — run
  `just nuttx build-fixtures-make`; not a product bug).

  **Resolved portion — the codegen bug (177.8.c).** All three **pubsub** cells
  are green: pub/sub issues no `z_get` query, so once the Rust nodes reach
  `main` (the release/lto fix, `8e5855c29`) they run clean. This confirms
  177.8.c is genuinely fixed for the codegen axis.

  **Correction 2026-05-26 — the "broad `z_get`/lease race" theory was WRONG.**
  The Cpp-action hang was a `fflush(stdout)` deadlock, not a `z_get`/lease race
  (177.30 Update 4 — fixed in `1804f7ce9`; full chain verified on a manual
  2-QEMU boot). Current matrix reality after 182.5 + the fflush fix:
  - **pubsub** — all 4 platforms green (never calls `z_get`).
  - **action** — NuttX + ThreadX-RISCV64 **removed from CI** (182.5, QEMU-heavy).
    NuttX Cpp action is fixed (fflush); NuttX C action was also dropped (the C
    example fflushes the same way — not retested, but out of the matrix).
  - **service** — still all 4 platforms in CI. NuttX **Cpp + C service PASS**,
    so there is **no general NuttX query/reply race** (a `z_get` race would sink
    Cpp/C service too). Only **NuttX service/Rust** flakes (flip pass/fail).
  **NuttX service/Rust — RESOLVED 2026-05-26.** Two distinct causes, both fixed:
  1. **Boot (fixture profile).** The local prebuilt Rust service fixtures
     existed only at `target/.../nros-fast-release/` (no `release/` build), and
     the harness falls back to that buggy profile (177.8.c CGU codegen bug →
     reboot loop before `main` → zero output → "Waiting for requests" never
     observed). `just nuttx build-fixtures` *does* build at `release`, so CI is
     fine; this was stale local artifacts. Rebuilt at `release`. The harness's
     `build_rust_example` fallback (`release` → `nros-fast-release`) now
     **eprintln-warns** when it takes the fast-release path, so a stale/partial
     local build is recognised instead of silently booting the known-broken
     profile with zero diagnostics
     (`packages/testing/nros-tests/src/fixtures/binaries/nuttx.rs`).
  2. **Round-trip race (the real one).** Reproduced in a clean manual 2-QEMU
     boot (C service round-trips perfectly with byte-identical keyexprs, so it
     was Rust-service-specific, not a general routing bug): the Rust client
     gates its first `call()` on `wait_for_service` (a *liveliness* token), but
     the server's *queryable* registration at the router lags that token, so
     the first query races ahead of the queryable and is dropped — `call(&req)?`
     then `promise.wait(…)?` aborts on the first `Transport(Timeout)`. The wire
     showed the query going client→router but the router never forwarding to the
     server. **Fix:** the Rust service-client example now retries the call
     (re-issuing once the queryable registers, `reset_in_flight()` between
     attempts), matching the blocking C client's internal robustness. Verified
     `[PASS]` 3/3 in nextest + a manual 2-QEMU boot (4/4 responses, ~30 s).
     The query/queryable keyexprs were never the issue.

  **Verification rerun caveat (2026-05-26) — residual connect-layer churn flake.**
  Isolated single-boot runs of the NuttX Rust e2e are reliable: service/Rust
  `[PASS]` **6/6** and pubsub/Rust green when run on their own. But a batch run
  (`(pubsub|service) & Nuttx`, all langs) and a rapid 5×-back-to-back boot loop
  both showed the Rust **server** intermittently failing `Executor::open` with
  `Transport(ConnectionFailed)` (1/5 connected in the rapid loop) → "readiness
  pattern never observed" boot failure. C/C++ servers were green throughout.
  This is a *connection-establishment* flake (zenoh-pico TCP connect to zenohd
  over slirp under rapid QEMU churn), **distinct from the round-trip race fixed
  above** and from any code I changed (the server is unmodified). It is the same
  "QEMU-under-load is brutal" sensitivity 182.5 cited when dropping NuttX
  *action* from CI; *pubsub + service* keep NuttX, so they remain exposed under
  a fully-parallel `test-all`. **Open follow-up (separate item):** make the Rust
  `Executor::open` connect retry/back off on `ConnectionFailed` (the C path is
  more tolerant) so the boot stage is churn-robust, OR give the NuttX Rust e2e
  more boot headroom / serialize its zenohd startup. Not the G4 round-trip bug.
- [x] **G5 - Native Cyclone DDS interop (4). RESOLVED 2026-05-26 — stale run.**
  `native_api::test_native_cyclonedds_{rust_talker_to_listener,talker_to_rust_listener}`
  for both `Language__C` and `Language__Cpp`. The failures were a mid-rebase
  artifact: the test-all clean-rebuild ran while `c2786def1` (177.26.RX.2) and
  the cyclone submodule bump to `12b4af2c` were still settling on main, so the
  fixtures linked an inconsistent Cyclone backend. Re-verified twice with main
  consistent — **all 4 PASS** (~8 s each, isolated). No code change needed.
  Tracked under **117 / 177.26** (both now reflect working native Cyclone
  pub/sub).
- [x] **G6 - XRCE runtime (2). RESOLVED 2026-05-26 — flake, not product bug.**
  `xrce::test_xrce_large_message_publish`, `xrce::test_xrce_service_request_response`.
  Reran under a focused XRCE filter as this entry asked (split flake from
  product bug): **both PASS** (~1.5 s + 5.8 s; the harness builds/launches its
  own agent). Same host-load discovery flake under the heavy parallel matrix as
  G3 / 177.9.H — not a product defect. The rest of the XRCE family (action
  fibonacci, multiple messages, serial) already passed.

**Conclusion.** Phase 181 build-SSOT is validated: clean rebuild green, migrated
fixtures (native + esp32-C3 + threadx-linux build-verified earlier; safety /
large-buf pass here), staleness probe clean post-build. The exit=1 is pre-existing
known-hard E2E. Follow-up 2026-05-26: G1/G2/G3 non-bugs (G2's 2 retired tests
deleted), **G5 resolved** (stale mid-rebase run — all 4 native Cyclone interop
tests pass with main consistent), **G6 resolved** (both XRCE tests pass on a
focused rerun — host-load flake, not a product bug), **G4 NuttX-Rust half
fixed** (177.8.c codegen, release/lto). **Only G4's NuttX-Cpp action remains**
(177.30 / 177.8.e — vendored zenoh-pico lease-task ↔ `z_get` timing race). Of
the original 20 test-all failures: G2 (−2) deleted, G5 (−4) + G6 (−2) stale/
flake, G4-Rust (−3) fixed → ~9 left, all the NuttX-Cpp action axis.

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
  Triage 2026-05-26: `logging_smoke` ×2 were a build-tier coverage gap,
  fixed under **177.8.a** (both now PASS). `ros2::*` ×2 + `integration_esp_idf`
  + `zpico_drift_gate` were 60 s TIMEOUTs (no panic) — env/contention, to
  re-run in isolation. `rtos_e2e` ×7: the 3 **ThreadxLinux Rust** failures are
  fixed under **177.8.b** (all 9 ThreadxLinux lang×variant now PASS); the 4
  **Nuttx** (Rust pubsub/service/action + Cpp action) remain open.

  - [x] **177.8.b - ThreadX-Linux Rust nodes SIGABRT on first `println!`
    after `Executor::open`.** All three ThreadxLinux Rust e2e
    (pubsub/service/action) failed `ensure_ready` (`rtos_e2e.rs:547`) — the
    node produced zero output (block-buffered pipe + abort discards it) and
    exited in ~2 s. Root cause (gdb + strace): `nros-platform-threadx`'s
    `src/platform.c` defines **weak POSIX stubs** (`open/close/read/write/
    lseek/pipe` + `stdin`) that unconditionally `return -1`. They are meant
    for *freestanding* bare-metal ThreadX (threadx-riscv64) where Cyclone's
    socket-waitset references those names and there is no libc. But the
    ThreadX *linux* port runs as a hosted Linux process linked to glibc, and
    a *weak* definition in the main executable still shadows glibc's public
    `write` for the dynamic lookup. Rust's `std::io::Stdout` calls the public
    `write` → stub's `-1` → `println!` panics "failed printing to stdout" →
    `panic = "abort"` → SIGABRT, before the readiness banner. C/C++ escape it
    because glibc stdio routes through the internal `__write` alias — that is
    the exact Rust-only asymmetry. Fix: gate the stubs behind
    `#if !defined(__linux__)` (the riscv64 cross toolchain doesn't define it;
    the hosted linux port does). Also fixed a latent build-hygiene bug:
    `nros-board-common::threadx_sources` emitted only a *directory*-level
    `cargo:rerun-if-changed` for the platform-threadx C sources, which does
    not fire on file-*content* edits — added per-file triggers for
    `platform.c`/`net.c`/`timer.c`. Verified: full ThreadxLinux matrix 9/9
    (Rust + C + C++ × pubsub/service/action) PASS; no regression to the
    C/C++ paths (which now link glibc's real `pipe`/`read`/`write` — exactly
    what Cyclone's self-pipe waitset wants).

  - [x] **177.8.c - NuttX-QEMU-ARM Rust nodes miscompiled at the fixture's
    optimization level (toolchain codegen bug; FIXED 2026-05-26).**
    **Fix:** build the NuttX Rust fixtures at the `release` profile
    (`opt-level="s"`, `lto=true`) instead of the default `nros-fast-release`
    (`lto=off`). Fat LTO merges the codegen units, so the *cross-CGU*
    fat-pointer miscompile cannot occur — a deterministic elimination of the
    mechanism, not a lucky flag flip, and it **preserves optimization**
    (opt-level=0 was rejected for the perf loss). Wired in `just/nuttx.just`
    (`build-examples` + `build-fixtures` build Rust via
    `NROS_CARGO_PROFILE=release`; C/C++ keep `nros-fast-release`) and the
    fixture lookup (`nuttx.rs`) now prefers the `release` artifact. **Verified:**
    `test_rtos_pubsub_e2e` nuttx/Rust PASSES (18 messages, clean); service +
    action nuttx/Rust PASS (flaky-pass on retry — the residual is the NuttX
    zenoh-pico `z_get` timing race, **177.8.e** class, not the codegen bug:
    the nodes now reach `main` and complete instead of booting silent). The C++
    action axis remains 177.8.e. Forensic trail that pinned the bug
    (gdb-multiarch remote to `qemu -s -S`, armv7a-nuttx-eabihf):
      * Entry chain is fine: `nxtask_startup → nsh_main` (our override) →
        `nsh_initialize()` (returns) → C `main` → `std::rt::lang_start` →
        `lang_start_internal`; `rt::init()` runs and returns.
      * At `rt.rs:175` `panic::catch_unwind(main)` dispatches the `&dyn Fn`
        main closure: `call_once` does `ldr r1,[r1,#0x14]; bx r1`, but the
        vtable/fat-pointer register holds garbage (`0x8`) → `bx` to ~NULL →
        executes zero-filled low memory → init task dies → reboot loop,
        *before any user output*. Raw `write(2)` works throughout (console
        is fine); the init-task stack is 512 KB (ample).
      * So a trait-object/closure fat-pointer is corrupt in optimized
        codegen, and it is **optimization-sensitive + non-deterministic**:
        `opt-level=0` (dev) reliably works (boots, prints the banner,
        reaches `Executor::open`); the fixture profile `nros-fast-release`
        (opt-level=2, codegen-units=16, lto=off, incremental, debug=1,
        panic=abort) reliably miscompiles. Bisection found *no single* knob
        is the trigger — `{panic=abort, debug=1, incremental}` break in any
        pair but not alone, `codegen-units=1` doesn't fix it, and identical
        logical flags flip WORKS/BROKEN across rebuilds — the signature of a
        non-deterministic cross-CGU/codegen miscompile in rustc/LLVM for
        this target, not a nano-ros logic bug.
      * C/C++ Nuttx examples are unaffected (they don't route through Rust
        `lang_start`'s closure dispatch).
    Candidate mitigations (need a decision): (a) build the NuttX Rust
    examples/fixtures at `opt-level=0` (correctness over size — verified to
    work, but large/slow, diverges from the shared profile); (b) hunt a
    deterministic safe optimized point / pin a known-good rustc; (c)
    escalate upstream as an armv7a-nuttx-eabihf codegen bug. Until then
    Nuttx Rust `rtos_e2e` stays red; Nuttx C/C++ pass. The Nuttx
    **Cpp-action** failure (`rtos_e2e.rs:844`) is a separate
    action-completion axis, not this codegen bug.

  - [x] **177.8.d - heavy subprocess tests TIMEOUT under full-suite load
    (not env/setup/fixture gaps).** The four group-C failures from the nuke
    gate — `ros2::tests::test_ros2_detection`, `..::test_rmw_fastrtps_detection`
    (lib unit tests), `integration_esp_idf::esp_idf_integration_shell_smoke`,
    `zpico_drift_gate::..._fires_on_corrupted_include` — all hit the 60s
    nextest terminate (30s × 2) as TIMEOUTs, not assertion failures.
    Investigated per the env/setup/fixtures angle and ruled all three out:
    ROS 2 *is* installed (`/opt/ros/humble`) with the `build/rmw_zenoh_ws`
    overlay; the ESP-IDF env is provisioned (`IDF_PATH` + the 177.7 env
    shim); none of these tests consume a `build-fixtures` artifact (they
    probe ROS 2 / run `idf.py build` / `cargo build -p zpico-sys` ×2
    themselves). Two distinct root causes (the second is a genuine
    resource conflict, not just CPU starvation):
      * **ros2 detection (lib `ros2::tests::*`) — ROS 2 daemon / DDS
        discovery contention.** `is_ros2_available` / `is_rmw_fastrtps_available`
        cold-start the ROS 2 CLI (`ros2 --help` / `ros2 pkg list`), which
        touches the *singleton* ROS 2 daemon + DDS discovery ports. The
        `ros2-interop` test-group is `max-threads = 1` precisely to serialize
        this ("parallel causes resource contention and discovery timeouts …
        daemon-sensitive"), but the *lib* detection copies were UNGROUPED, so
        they ran concurrently with each other and `rmw_interop` → daemon/
        discovery contention → 60s hang. Fix: assign them to `ros2-interop`
        (the group serialization kills the conflict); the `120s × 3` budget
        is belt-and-suspenders. Scoped to `ros2::tests::` only so the cheap
        binary-existence probes (zenohd/cmake/west/espflash) are NOT
        force-serialized.
      * **esp_idf / zpico_drift_gate — CPU/IO-bound cold builds, no resource
        conflict.** `idf.py build` binds no ports; drift_gate builds into a
        dedicated `target-zpico-drift-gate/` dir (no cargo target-lock
        contention). They simply exceed the 60s default when every core is
        busy → a `120s × 3` slow-timeout bump (no group) is enough.
    Fix lands in `.config/nextest.toml`. Verified all four PASS in isolation
    (ros2 detection ~0.3s, esp_idf ~35s, drift_gate ~26s).

  - [x] **177.8.e - NuttX-QEMU-ARM Cpp action goal never reaches the server —
    RESOLVED (`fflush` deadlock; see 177.30 Update 4).** The "client prints
    `Sending goal` then nothing" symptom was the app thread blocking in the
    `fflush(stdout)` right after that print (libc stdout `FILE*` lock vs
    zenoh-pico bg threads) — it never reached `send_goal_async`, so the query
    was never sent. The "z_get blocks and never returns" note below was the
    mistaken read (gdb parked the thread *in fflush*, upstream of z_get). Fixed
    by dropping the example's `fflush` calls (`1804f7ce9`); full chain verified
    on a manual 2-QEMU boot. Original investigation preserved below.
    `test_rtos_action_e2e` Nuttx/Cpp: the client prints
    `Sending goal: order=5`, then nothing — `goal_accepted=false`, and the
    server's post-boot log is empty (it sits at "Waiting for goals" and never
    logs a goal request). So the **send_goal query never reaches the server**.
    Nuttx Cpp **pub/sub + service** pass and the Nuttx **C** action passes, so
    the transport, service request/reply, and the server's action queryable
    all work in isolation — it is specific to the Cpp action goal path.
    Investigation (extensive, 2026-05-26):
      * The Cpp `send_goal_async` AND the "blocking" `send_goal` both route
        through `core.send_goal_raw` → `ServiceClient::send_request_raw` →
        `zpico_get_start` → `z_get`, which **blocks and never returns** on
        NuttX zenoh-pico when issued outside a spin loop (gdb: app thread
        parked; the fflush'd print after the send call never fires). The
        `send_request_raw` std deadline-retry loop is bounded, so the hang is
        inside `z_get` itself, not the loop.
      * Rerouting the Cpp blocking `send_goal` to `core.send_goal_blocking`
        (the `call_raw` path that Cpp **service** + the C action client use
        successfully on NuttX) was verified to link (disasm shows `call_raw`,
        not `send_goal_raw`) — but the client **still** hangs at "Sending
        goal" and the server still receives nothing. So even `call_raw` to the
        action `send_goal` queryable does not deliver on NuttX-Cpp, despite
        the identical `call_raw` working for Cpp service and for the C action
        client. Root cause not yet isolated (candidate: action send_goal
        queryable keyexpr match / discovery between the Cpp client and Cpp
        server on NuttX, or a NuttX-specific zenoh-pico get/query interaction).
      * Build caveat learned: a bare `cmake --build <build-zenoh>` does NOT
        apply the justfile's stale-`nros-*`-fingerprint guard (justfile:259),
        so nros-cpp edits silently kept a stale rlib — a full `cargo-target`
        wipe (or the `just` recipe) is required to retest core-crate changes
        for a cmake fixture.
    All exploratory edits reverted; tree + fixture left at the committed
    state. Tracks with the other open NuttX items (177.8.c); needs focused
    NuttX zenoh-pico action-path work.

    **Wire + gdb follow-up (2026-05-26, tshark on lo:7672 + gdb-multiarch):**
      * tshark proves the **goal query never reaches the wire**: after
        discovery (~32 s — both endpoints declare liveliness for
        `…/_action/send_goal`, server `SS`, client `SC`), the two
        guest↔zenohd TCP connections carry **only 3-byte keep-alives**, no
        query frame. So the client blocks in the send path *before*
        transmitting (consistent with gdb: the app thread is parked).
      * gdb call chain at the block: `nros_app_main → send_goal_async →
        send_goal_raw → CffiServiceClient::send_request_raw →
        zpico_get_start → z_get → _z_query → _z_send_n_msg`. Tracing
        `_z_mutex_lock` shows the **lease task**
        (`_zp_unicast_lease_task → _z_pending_query_process_timeout`)
        contending the session/pending-query mutex against the app thread's
        `z_get` send + reply-final path.
      * **It's a timing race, not a hard hang.** With the server present and
        the client run *under gdb* (perturbed timing), `z_get` RETURNS and
        the server logs `Goal request [1]: order=5` — the action completes.
        At native speed the lease task and `z_get`'s `_z_send_n_msg` /
        reply-final processing deadlock. Note `_z_query` itself unlocks the
        session mutex (`primitives.c:542`) *before* `_z_send_n_msg`
        (`:558`), so it is not a session-mutex AB-BA inside `_z_query`; the
        cycle is between the TX path and the lease task's session-locked
        timeout sweep. The action client hits it (and pub/sub + service
        mostly don't) because it carries far more concurrent pending-query
        churn — send_goal + get_result + cancel_goal clients plus
        feedback/status subs, with the warm-up `poll()` issuing queries the
        lease task is timing out exactly when send_goal's `z_get` fires.
    Fix is a vendored zenoh-pico (1.7.2) lock-ordering change between the
    unicast lease task's `_z_pending_query_process_timeout` and the query
    TX / reply-final paths — cross-platform-risky, so left for a focused
    zenoh-pico concurrency task, not landed here. **Promoted to first-class
    item 177.30** (root cause + fix roadmap + acceptance); this entry is the
    investigation log.
- Two build-all-after-clean fragilities surfaced by the nuke gate:
  - **(fixed, `6e1d26dee`)** jobserver prefetch ran `cargo fetch
    --locked` on standalone example/fixture dirs whose gitignored
    `Cargo.lock` goes stale after a clean+setup → hard-fail. Dropped
    `--locked` in `scripts/build/cargo.sh::nros_cargo_fetch_standalone_manifests`.
  - **(fixed)** `build-all-jobserver` prefetch assumed example
    `generated/` dirs exist, but they are gitignored and a clean wipes
    them; the standalone-manifest prefetch (`cargo fetch` in dirs whose
    `.cargo/config.toml` `[patch.crates-io]` points at `generated/<pkg>`)
    hard-failed `unable to update generated/<pkg>` → `No such file or
    directory`. Root cause: codegen (`just generate-bindings` + the
    per-platform `ensure_native_rust_generated` steps) only ran *inside*
    `build-all.mk`, but the standalone prefetch ran in the recipe *before*
    the mk — and the fetches must stay there because the mk runs
    `CARGO_NET_OFFLINE=true`. Fix: run `just generate-bindings` (the
    existing incremental global Rust-example codegen sweep) in the recipe
    ahead of `nros_cargo_fetch_standalone_manifests`. C/C++ examples never
    hit this — their codegen is a cmake step. The mk keeps generate-bindings
    as a prereq for the direct `make -f build-all.mk` path; the incremental
    helper makes the recipe's earlier pass cheap on a warm tree. Verified
    by wiping one example's `generated/` (`cargo fetch` rc=101), running
    `generate-bindings`, re-fetching (rc=0).

## Archive Rule

Archive this tracker only after:

- [x] 177.3 closes or moves into a newer, more specific phase doc.
- [x] 177.6 through 177.9 have owners and either close or move into more
  specific phase docs. (2026-05-26: 177.6/177.7/177.9 closed; 177.8 closed with
  its residual layout goal moved to Phase 181 fixture-build SSOT.)
- [x] 177.19 and 177.20 close or move into platform-specific runtime
  phases.
