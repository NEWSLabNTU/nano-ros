# Phase 97 — DDS per-platform examples + cross-platform E2E

**Goal**: Close the example + per-platform-E2E half of the original
Phase 71 (dust-dds platform-agnostic) effort. Phase 71's
infrastructure block landed end-to-end (cooperative runtime, async
transport, size-probed buffers, smoltcp multicast, POSIX validation,
generic global-allocator) but the eight per-platform DDS pubsub
talker / listener crates and their nros-tests fixtures are still
open. Each per-platform slice is a from-scratch board bring-up
exercise — too big to fit alongside Phase 71's infrastructure, hence
splitting it out.

**Status**: In Progress — 4 of 7 97.4 slices green
(`freertos`, `nuttx`, `threadx-linux`, `threadx-riscv64`); exceeds the
"≥3 of 7" acceptance threshold. Remaining: `zephyr-native_sim`
(blocked upstream NSOS), `baremetal` + `esp32-qemu` (gated by 97.3
bare-metal smoltcp DDS bring-up).

**Priority**: Medium. Native + Zephyr (`qemu_cortex_a9`) DDS already
ship and cover the user-visible surface. Each remaining per-platform
slice is "another cross-compile target works for DDS" — incremental
coverage rather than a new feature.

**Depends on**:
- Phase 71 infrastructure block: cooperative runtime (71.1–71.5),
  async transport API port (71.4.b), `PlatformUdp::listen` per
  platform (71.20 / 71.21), size-probed buffers (71.22),
  shared Kconfig matrix (71.23 doc), POSIX loopback validation
  (71.24), `PlatformUdp` contract pinned (71.25 doc), smoltcp
  multicast bridge + macro (71.26), `nros-platform/global-allocator`
  feature (71.27 prereq).
- Phase 71.28 / 71.29: bug-fix follow-ups (slice-offset in
  `handle_request`; cooperative-runtime starvation in DDS A9
  example clients). Both closed.

## Architecture / design

The remaining work splits into three concentric layers:

1. **Generic 71.27 prerequisites** — finish the four pieces every
   per-platform DDS example will need:
   - `nros-platform/critical-section` cargo feature with per-RTOS
     `critical_section::Impl` (FreeRTOS / NuttX / ThreadX / smoltcp).
     The Zephyr impl already exists in `nros-c`'s
     `cfg(feature = "platform-zephyr")` block — lift the shape into
     `nros-platform` so it's available to pure-Rust examples too.
     dust-dds's oneshot channels reference
     `_critical_section_1_0_acquire / _release`; without an impl,
     every per-platform example fails to link.
   - Per-board linker scripts: RAM region big enough for dust-dds
     rodata. `.ARM.extab` placement on Cortex-M FreeRTOS overlapped
     `.data` in the first bring-up attempt (Phase 71 archived
     notes); document the typical sizing per board (~700 KB code,
     ~100 KB rodata).
   - Per-board heap config bumped to ≥ 256 KB (`configTOTAL_HEAP_SIZE`,
     `CONFIG_MM_KERNEL_HEAPSIZE`, `tx_byte_pool_create` pool, …).
   - Per-RTOS Kconfig delta (the matrix documented in
     `book/src/user-guide/rmw-backends.md` "DDS — per-platform
     configuration profile" — copy the appropriate snippet into the
     example's `prj.conf` / `lwipopts.h` / `nx_user.h`).

2. **Per-platform PlatformUdp smoke binary** — single-process bind /
   send / recv self-test mirroring Phase 71's POSIX loopback suite.
   Useful when a board first comes up and the full pubsub stack would
   mask which layer is broken. Each slice is a 200-line `lib.rs` plus
   the matching board-crate cargo / linker glue.

3. **Per-platform DDS pubsub E2E** — talker + listener crate pair
   plus a `tests/<rtos>.rs` entry in `nros-tests` that spawns two
   QEMU instances on a shared netdev (slirp / mcast socket) and
   asserts `Received: <data>` is logged on the listener side within
   a timeout. Every slice depends on the corresponding 71.27.cs /
   linker / heap / Kconfig pieces.

The 71-numbered sub-bullets used during Phase 71 are renumbered as
97-prefixed items below; cross-references in
`docs/roadmap/archived/phase-71-dust-dds-platform-agnostic.md` and
in `book/src/user-guide/rmw-backends.md` keep working through the
matrix in the book.

## Work Items

### 97.1 — Generic prerequisites (gate every per-platform example)

- [~] **97.1.cs** — `nros-platform/critical-section` cargo feature
      + per-RTOS `critical_section::Impl`. **FreeRTOS slice landed**:
      `nros-platform-freertos/src/lib.rs` ships a Cortex-M PRIMASK
      impl behind `feature = "critical-section"`, registered via
      `critical_section::set_impl!`. `nros-platform`'s
      `critical-section` feature forwards through. Verified by
      `cargo build -p nros-platform-freertos --features critical-section
      --target thumbv7m-none-eabi`. Per-RTOS slices below still need
      their own impl:
      - [x] **97.1.cs.freertos** — Cortex-M PRIMASK via `cortex-m`.
      - [ ] **97.1.cs.nuttx** — `up_irq_save` / `up_irq_restore`.
      - [ ] **97.1.cs.threadx** — `__disable_irq` / restore.
      - [ ] **97.1.cs.smoltcp** — `cortex-m::interrupt::free`
            (same as freertos for Cortex-M boards; different impl
            for RISC-V / ESP32 if they ever land).
      - Zephyr already provides this via `nros-c`'s `ZephyrCs`;
        the Phase 97 examples on Zephyr that don't go through
        `nros-c` would need this lifted into `nros-platform-zephyr`.
- [~] **97.1.linker** — document RAM-region sizing per board.
      MPS2-AN385 slice landed (Cortex-M3 + FreeRTOS): root cause was
      a missing `.ARM.extab` placement in
      `packages/boards/nros-mps2-an385-freertos/config/mps2_an385.ld`.
      dust-dds emits `.ARM.extab` entries for its panic unwind paths;
      without an explicit output section the linker dropped them at
      the start of `.data`, overlapping initialised data. Fix: a
      one-line `.ARM.extab` section in FLASH between `.text` and
      `.ARM.exidx`. Other boards likely need the same audit:
      - [x] **97.1.linker.mps2-an385-freertos** — fixed inline with
            97.4.freertos talker / listener bring-up.
      - [ ] **97.1.linker.mps2-an385** (bare-metal smoltcp).
      - [ ] **97.1.linker.stm32f4**.
      - [ ] **97.1.linker.esp32-qemu**.
- [ ] **97.1.heap** — heap config delta per board (Cargo.toml
      feature gate + `prj.conf` / `FreeRTOSConfig.h` /
      `tx_user.h` / `nx_user.h` patch). Lands inline with each
      example's Cargo manifest.
- [~] **97.1.kconfig** — pull each `[71.23.<plat>]` block from the
      book into the matching example's
      `prj.conf` / `lwipopts.h` / `nx_user.h`. The matrix is
      documented; this is mechanical copy.
      - [x] **97.1.kconfig.freertos** — landed in
            `packages/boards/nros-mps2-an385-freertos/config/lwipopts.h`:
            `LWIP_IGMP=1`, `LWIP_BROADCAST=1`, `IP_REASSEMBLY=1`,
            `MEMP_NUM_NETBUF` bumped 8→32. `igmp.c` added to the
            board's `build.rs` lwIP source list. Shared config —
            zenoh examples link unchanged (verified: 9/9 FreeRTOS
            rtos_e2e zenoh tests still pass).
      - [ ] **97.1.kconfig.nuttx** — `CONFIG_NET_IGMP=y`,
            `CONFIG_NET_BROADCAST=y`, `CONFIG_NET_UDP_NRECVS≥4`,
            `CONFIG_NET_RECV_TIMEO=y`. Lands with 97.4.nuttx.
      - [ ] **97.1.kconfig.threadx** — `NX_ENABLE_IGMPV2` + NetX
            BSD-layer `SO_RCVTIMEO` init. Lands with 97.4.threadx-*.
      - [ ] **97.1.kconfig.smoltcp** — `MulticastConfig::Strict` +
            `Interface::join_multicast_group`. Already addressed by
            Phase 71.26's bridge / macro; per-board wiring lands
            with 97.4.baremetal / 97.4.esp32-qemu.
- [x] **97.1.board-decouple** — board crates currently hard-call
      zenoh-pico-specific symbols at boot (`zpico_set_task_config`,
      `extern crate zpico_*`). DDS-only builds reach the linker
      step with these symbols undefined. Fix: option (a) — cfg-gate
      the calls behind a `feature = "rmw-zenoh"` on the board crate;
      keep the priority / stack config fields on `Config` because the
      same FreeRTOS-priority knobs (`zenoh_read_priority` /
      `zenoh_lease_priority` etc.) tune zenoh-pico's read / lease
      tasks to avoid priority inversion against the app and poll
      tasks. The fields are zenoh-named for historical reasons but
      are config data, not RMW linkage; they only get pushed into
      zenoh-pico when the matching feature is active.
      - [x] **97.1.board-decouple.mps2-an385-freertos** — landed.
            `nros-mps2-an385-freertos`'s `Cargo.toml` makes
            `zpico-platform-shim` + `zpico-sys` optional under a new
            `rmw-zenoh` feature (defaulted on for backward compat);
            `lib.rs`'s `extern crate zpico_*` lines and `node.rs`'s
            `zpico_set_task_config` block are both cfg-gated.
            Verified: existing zenoh examples still link unchanged
            (default features keep the historic shape); the new DDS
            talker / listener depend with `default-features = false`
            and link cleanly without the zenoh-pico symbol set.
      - [x] **97.1.board-decouple.mps2-an385** — landed (bare-metal
            smoltcp). Same shape: optional `zpico-platform-shim`,
            new `rmw-zenoh` feature in `default`,
            cfg-gated `extern crate zpico_platform_shim`. Verified
            both feature combos build for `thumbv7m-none-eabi`;
            existing `examples/qemu-arm-baremetal/rust/zenoh/talker`
            still builds clean.
      - [x] **97.1.board-decouple.stm32f4** — landed. Same template;
            `default = ["stm32f429", "ethernet", "rmw-zenoh"]`.
      - [x] **97.1.board-decouple.esp32-qemu** — landed. Same
            template; `default = ["ethernet", "rmw-zenoh"]`.
      - [x] **97.1.board-decouple.nros-nuttx-qemu-arm** — no-op,
            board crate already clean (NuttX userspace links libc;
            zenoh-pico runs against POSIX socket API, no force-link
            of zpico-platform-shim).
      - [x] **97.1.board-decouple.threadx-qemu-riscv64** — no-op,
            same reason as NuttX.
      - [x] **97.1.board-decouple.threadx-linux** — no-op, same
            reason as NuttX.

### 97.2 — Per-platform PlatformUdp smoke binary

Each slice ports the Phase 71.24 POSIX loopback contract to a
cross-compile QEMU configuration. Single-process bind / send / recv;
no peer process. Useful for board bring-up debugging before the full
DDS stack is wired.

- [ ] **97.2.zephyr-native_sim** — blocked behind upstream NSOS
      `IP_ADD_MEMBERSHIP` gap (see archived 71.8 note).
- [ ] **97.2.freertos** — qemu-arm-freertos / MPS2-AN385.
- [ ] **97.2.nuttx** — qemu-arm-nuttx.
- [ ] **97.2.threadx-riscv64** — qemu-riscv64-threadx.
- [ ] **97.2.threadx-linux** — ThreadX Linux sim.
- [ ] **97.2.baremetal** — MPS2-AN385 (smoltcp).
- [ ] **97.2.esp32-qemu** — ESP32-QEMU.

### 97.3 — Bare-metal DDS talker / listener examples

Bare-metal examples need 71.26.qemu (smoltcp IGMP E2E smoke) to land
first; until then the multicast SPDP path is proven only in the unit
tests landed by Phase 71.26.

- [~] **97.3.mps2-an385** — `examples/qemu-arm-baremetal/rust/dds/`
      talker + listener. Build path lands green:
      - Example crates build clean for `thumbv7m-none-eabi` with
        `nros = features = ["alloc", "rmw-dds", "platform-bare-metal",
        "ros-humble"]`. Single-instance smoke runs end-to-end —
        talker reaches `Publisher declared` and publishes, listener
        reaches `Subscriber declared` and enters its recv loop.
      - `nros-rmw-dds`'s previously-omitted `platform-bare-metal`
        Cargo feature now exists (forwards `nostd-runtime`); `nros`'s
        `platform-bare-metal` propagates it.
      - `nros-platform-mps2-an385`: new `dds-heap` feature bumps the
        free-list heap 64 KB → 2 MB so dust-dds's `DcpsDomainParticipant`
        builtin entities fit. New `critical-section` feature flips
        `cortex-m/critical-section-single-core` so the
        `_critical_section_1_0_*` symbols dust-dds references resolve.
      - `nros-board-mps2-an385`: re-exposes the heap bump under a
        `dds-heap` Cargo feature; provides `smoltcp_clock_now_ms`
        directly (cfg-gated to non-zenoh builds) since zpico-platform-
        shim is dropped. `[dds] domain_id = N` accepted in
        `Config::from_toml`.
      - `nros-smoltcp`: `MAX_UDP_SOCKETS` default bumped 2 → 4 (RTPS
        needs 3 per participant — default-unicast + metatraffic-unicast
        + metatraffic-multicast); UDP_*_2 / UDP_*_3 static buffers
        added. `set_recv_timeout(_, 0)` no longer silently falls back
        to the 10 s `SOCKET_TIMEOUT_MS` default — passes 0 verbatim,
        which is what the cooperative recv loops want.
      - `lan9118-smoltcp`: `enable_mac_rx` now flips `MCPAS` (Pass All
        Multicast) + `PRMS` (Promiscuous) so SPDP `01:00:5e:7f:00:01`
        frames reach smoltcp without per-group hash filter
        programming. Same fix the FreeRTOS lan9118-lwip slice landed.
      - Cross-instance E2E **NOT** yet green: talker + listener on a
        shared `-nic socket,mcast=…` segment both emit RTPS frames
        (host-side `socat` confirms both flows visible on
        loopback), but neither side's `multicast_recv_loop` ever
        fires. Open suspect: smoltcp's IPv4 mcast RX dispatch under
        the bridge's `iface.join_multicast_group` queue isn't
        delivering inbound mcast frames to the bound UDP socket.
        Needs further smoltcp-trace work before 97.4.baremetal can
        close.
      - `tmp/baremetal-dds-{2-instance,smoke,tshark}.sh` ship as
        repro scripts.
- [ ] **97.3.esp32-qemu** — `examples/qemu-esp32-baremetal/rust/dds/`
      talker + listener.

### 97.4 — Per-platform DDS pubsub E2E

Talker + listener + nros-tests fixture. Each slice depends on the
matching 97.1 prerequisites and (for bare-metal) 97.2.baremetal /
97.2.esp32-qemu.

- [ ] **97.4.zephyr-native_sim** — blocked behind NSOS gap.
- [x] **97.4.freertos** — qemu-arm-freertos talker↔listener.
      `test_freertos_dds_rust_talker_to_listener_e2e` passes
      end-to-end (~83 s) on QEMU MPS2-AN385 + lwIP. Path:
      - Talker + listener crates at
        `examples/qemu-arm-freertos/rust/dds/{talker,listener}/`.
      - `QemuProcess::start_mps2_an385_mcast` launcher (no
        `localaddr` — host kernel picks the primary iface so
        sibling QEMUs deliver each other's mcasts).
      - `build_freertos_dds_{talker,listener}` fixtures in
        `nros-tests/src/fixtures/binaries/freertos.rs`.
      - `.config/nextest.toml` routes `freertos_qemu_dds` into the
        existing `qemu-freertos` test-group (120 s slow-timeout,
        2 retries).

      Bring-up debt closed in this phase:
      - `nros-platform-freertos::net.rs` `mcast_*` real impls
        (was stub-returning -1) — `IP_ADD_MEMBERSHIP` setsockopt,
        `O_NONBLOCK` fcntl for cooperative recv loops.
      - `udp_create_endpoint` `AI_NUMERICHOST` flag (RTPS literals
        skip the unconfigured DNS resolver).
      - `lan9118_lwip.c` — `NETIF_FLAG_IGMP` + MAC_CR `MCPAS` so
        IGMP-joined groups actually reach lwIP.
      - `lwipopts.h` — `MEMP_NUM_NETDB` 1 → 16 (every
        `udp_create_endpoint` allocates an `addrinfo`).
      - `FreeRTOSConfig.h` — `configTOTAL_HEAP_SIZE` 256 → 2048 KB
        (DcpsDomainParticipant builtin entities use ~512 KB).
      - bindgen allowlist — `IP_ADD_MEMBERSHIP`, `ip_mreq`,
        `in_addr`, `INADDR_ANY`, `AI_NUMERICHOST`.
      - `NROS_LOCAL_IPV4` per-example via `.cargo/config.toml` so
        each guest advertises its own iface IP in SPDP unicast
        locators *and* uses those bytes as the dust-dds `host_id`
        in the GUID prefix — without distinct prefixes both peers
        would self-filter each other's SPDP and SEDP would never
        close.
      - `debug-cortex-m-semihosting` feature on `nros-rmw-dds` /
        `nros-platform-freertos` — gated, off by default; turns
        on a step-by-step Cortex-M semihosting trace through every
        bind / write / recv for the next platform's bring-up.
- [x] **97.4.nuttx** — qemu-arm-nuttx talker↔listener.
      `test_nuttx_dds_rust_talker_to_listener_e2e` passes end-to-end
      (~83 s) on QEMU `-M virt -cpu cortex-a7` + NuttX POSIX socket
      stack + virtio-net-device. Path:
      - Talker + listener crates at
        `examples/qemu-arm-nuttx/rust/dds/{talker,listener}/`.
      - `QemuProcess::start_nuttx_virt_mcast` launcher (no
        `localaddr` — same lesson as the FreeRTOS slice).
      - `build_nuttx_dds_{talker,listener}` fixtures in
        `nros-tests/src/fixtures/binaries/nuttx.rs`.
      - `.config/nextest.toml` extends `qemu-nuttx` test-group
        filter to include `binary(nuttx_qemu_dds)`.

      Bring-up debt closed in this slice (delta vs Phase 97.4.freertos):
      - `nros-platform-nuttx::net.rs` `mcast_*` real impls (was
        stub-returning -1) — `IP_ADD_MEMBERSHIP` + `O_NONBLOCK`
        fcntl, same shape as the FreeRTOS path.
      - `udp_create_endpoint` `AI_NUMERICHOST` flag.
      - `nuttx-config/defconfig` — `CONFIG_NET_IGMP=y` +
        `CONFIG_NET_IGMPv2=y`.
      - `nuttx-sys` bindgen allowlist — `IP_ADD_MEMBERSHIP`,
        `IP_DROP_MEMBERSHIP`, `INADDR_ANY`, `IPPROTO_IP`,
        `AI_NUMERICHOST`, `ip_mreq`, `in_addr`.
      - `nros-board-nuttx-qemu-arm::node.rs` — `apply_ip_config`
        helper that drives `SIOCSIFADDR / SIOCSIFNETMASK /
        SIOCSIFDSTADDR` from `Config.ip / prefix / gateway` so
        sibling QEMU guests don't both default to the
        `CONFIG_NETINIT_IPADDR` baked into `defconfig`. The ioctl
        request numbers use NuttX's `_SIOC(N) = 0x0700|N` encoding
        — *not* the Linux `0x89xx` range that a quick port from
        glibc would use.
      - `NROS_LOCAL_IPV4` per-example via `.cargo/config.toml`,
        same role as the FreeRTOS slice (SPDP unicast locator +
        dust-dds GUID-prefix host_id seed).
      - Examples select `alloc` (not `std`) on `nros` so the
        dust-dds `nostd-runtime` path is taken instead of the
        `rtps_udp_transport` socket2 path that fails to compile
        against the NuttX-flavoured libc (no `SO_REUSEPORT`,
        `IovLen`, …). The example pulls `critical-section/std`
        directly to satisfy `_critical_section_1_0_*` references
        emitted by dust-dds's `MpscReceiverFuture` poll path.
      - New `debug-stderr` feature on `nros-rmw-dds` mirrors
        `debug-cortex-m-semihosting` for std-capable platforms
        (NuttX, ThreadX-Linux, native_sim) — same trace points,
        routed through `eprintln!` instead of Cortex-M
        semihosting.
- [x] **97.4.threadx-riscv64** — qemu-riscv64-threadx
      talker↔listener. End-to-end green: ~870 messages received in
      the 60 s test window across two QEMU `-netdev socket,mcast=…`
      instances:
      - Example crates at
        `examples/qemu-riscv64-threadx/rust/dds/{talker,listener}/`,
        both build clean for `riscv64gc-unknown-none-elf`.
      - `nros-platform-threadx` `mcast_*` impls (NetX Duo's BSD
        `IP_ADD_MEMBERSHIP` + `nx_bsd_fcntl(O_NONBLOCK)`).
      - `nros-platform-threadx` `critical-section` feature with a
        RISC-V impl that toggles `mstatus.MIE` via inline asm —
        same shape as the FreeRTOS Cortex-M PRIMASK impl.
      - `nx_user.h` enables `NX_ENABLE_IGMPV2` for the SPDP join.
      - Board byte pool bumped 512 KB → 2 MB to host
        `DcpsDomainParticipant` builtin entities.
      - `QemuProcess::start_riscv64_virt_mcast` launcher.
      - `build_threadx_rv64_dds_{talker,listener}` fixtures.
      - `tests/threadx_riscv64_qemu_dds.rs` integration test
        (`test_threadx_rv64_dds_rust_talker_to_listener_e2e`)
        passes via nextest in ~83 s.

      Root cause: `udp_set_recv_timeout` /
      `udp_listen_inner(timeout_ms>0)` /  `mcast_listen(timeout_ms=0)`
      were calling `nx_bsd_setsockopt(SO_RCVTIMEO, &INT, 4)`, but
      `nxd_bsd.c` casts the option buffer directly to
      `struct nx_bsd_timeval *` (8 bytes on RV64 LP64). The
      mismatch silently produced `option_receive_timeout = 0`,
      which falls through to `wait_option = NX_WAIT_FOREVER` and
      lets the cooperative recv loop's first `nx_bsd_recv` block
      the entire app thread. Switched the cooperative-non-blocking
      path to `nx_bsd_fcntl(F_SETFL, O_NONBLOCK)` (NetX BSD F_SETFL
      toggles `NX_BSD_SOCKET_ENABLE_OPTION_NON_BLOCKING` on the
      socket entry, and the recv path honours that flag) and the
      finite-timeout path to a real `nx_bsd_timeval` struct.
- [x] **97.4.threadx-linux** — ThreadX Linux sim talker↔listener.
      `test_threadx_linux_dds_rust_talker_to_listener_e2e` passes
      end-to-end (~123 s) on Linux loopback + NSOS + NetX BSD
      shim. talker → listener E2E green: 26 messages received in
      the steady-state window. Two unrelated bugs landed together
      to flip this from red to green:
      - Example crates at
        `examples/threadx-linux/rust/dds/{talker,listener}/`,
        both build clean for `x86_64-unknown-linux-gnu`.
      - `nros-platform-threadx` shares the `mcast_*` impls used
        by the qemu-riscv64-threadx slice (NetX Duo BSD shim
        `IP_ADD_MEMBERSHIP` + `nx_bsd_fcntl(O_NONBLOCK)`).
      - `nx_user.h` enables `NX_ENABLE_IGMPV2` so NetX BSD's
        `IP_ADD_MEMBERSHIP` setsockopt actually fires.
      - `nsos-netx::nsos_netx.c` `translate_sockopt()` translates
        NetX-BSD `IPPROTO_IP=2`, `IP_*MEMBERSHIP=32/33`,
        `IP_MULTICAST_*=27/28/29` to the Linux kernel's
        `IPPROTO_IP=0`, `IP_*MEMBERSHIP=35/36`,
        `IP_MULTICAST_*=32/33/34` so NSOS's verbatim
        `setsockopt`/`getsockopt` forwarding doesn't fail with
        `ENOPROTOOPT` for the multicast knobs that DDS relies on.
      - `tcp_create_endpoint` no longer rejects `0.0.0.0`
        (RTPS unicast + multicast listens use it for any-iface).
      - Board byte pool bumped 512 KB → 2 MB (same DDS heap
        budget as the FreeRTOS / RV64 slices).
      - Examples select `alloc` (not `std`) on `nros` so dust-dds
        runs the cooperative `nostd-runtime` path — saves the
        socket2 compile failures that hit the NuttX slice and
        keeps the platform-threadx code path consistent across
        no_std (qemu-riscv64) and std (Linux sim) deployments.
      - `critical-section = { features = ["std"] }` direct dep
        on each example, same as NuttX.
      - `QemuProcess`-style test fixtures + `tests/
        threadx_linux_dds.rs` integration test land.

      Runtime SPDP + SEDP exchange now flows end-to-end on
      127.x.y.z loopback (config.toml updated to `127.0.10.10` /
      `127.0.10.11` with domain_id `42` to dodge the host's
      default-domain DDS noise; `nsos-netx::translate_sockopt`
      additionally now converts NetX BSD's `INT`-millisecond
      `SO_RCVTIMEO` / `SO_SNDTIMEO` into Linux `struct timeval`
      so the cooperative recv loops don't end up blocking 1 second
      per `nx_bsd_recv` because Linux read the INT as `tv_sec`).
      Both sides exchange SPDP and SEDP unicast cleanly.

      Final fixes in this slice (both also benefit every other
      cooperative-`nostd-runtime` deployment):

      1. **dust-dds heartbeat_period 200 ms → 2000 ms** in
         `rtps/stateful_writer.rs`. On Linux loopback's
         ~zero-latency RTT the 200 ms cycle saturated the
         cooperative single-thread poll loop with AckNack ↔ DATA
         ping-pong; talker stalled at "Published: 5". 2 s gives
         the cooperative scheduler room to drain matched-reader
         bookkeeping between heartbeats. ROS 2 / Cyclone /
         FastDDS defaults are 1 s+ for the discovery writer, so
         this is in line with upstream. Web research:
         [DeepWiki dust-dds RTPS](https://deepwiki.com/s2e-systems/dust-dds/9-rtps-protocol-and-transport),
         [Cyclone DDS DDSI docs](https://cyclonedds.io/docs/cyclonedds/0.10.2/config/ddsi_concepts.html).

      2. **Drop `SO_REUSEADDR` on the unicast `udp_listen`** path
         in `nros-platform-threadx::net.rs`. Two co-resident
         ThreadX-Linux processes both bound to
         `INADDR_ANY:default_unicast_port`; with `SO_REUSEADDR`
         the kernel delivered each /chatter datagram to *one* of
         them on a round-robin → the listener missed every other
         RTPS message and the SEDP handshake never closed.
         Without it, the second process's `bind(2)` returns
         `EADDRINUSE` and the `create_participant` auto-pid loop
         (already there, never fired) bumps to the next slot, so
         each participant gets a unique unicast port. Multicast
         binds layer the option back in via the dedicated
         `udp_listen_reusable` helper so SPDP joins still
         succeed.
- [ ] **97.4.baremetal** — MPS2-AN385 talker↔listener (depends on
      97.3.mps2-an385).
- [ ] **97.4.esp32-qemu** — ESP32-QEMU talker↔listener (depends on
      97.3.esp32-qemu).

### 97.5 — Optional follow-ons

- [ ] **97.5.cyclone-fastdds-interop** — *(optional)* CycloneDDS /
      FastDDS interop test in nros-tests. Independent of the per-
      platform matrix; useful regression coverage once at least one
      cross-compile platform's E2E is stable.
- [ ] **97.5.upstream-transport** — *(optional)* upstream
      `NrosUdpTransportFactory`'s non-blocking transport back to
      dust-dds. Removes the local fork dependency on dust-dds without
      blocking any nano-ros work. Shape stable since Phase 71.22's
      size-probed buffers landed.

## Acceptance Criteria

- [ ] **97.1.cs** + the four other 97.1 prerequisites each have at
      least one consumer (the matching 97.4 slice) building cleanly
      against them.
- [ ] **97.2** — at least one per-platform `PlatformUdp` smoke binary
      lands and runs green in `just <plat> test`. Remaining slices
      can copy the template.
- [ ] **97.4.freertos** lands as the canonical "first non-Zephyr DDS
      RTOS" example, exercising every 97.1 piece end-to-end. Other
      RTOS slices template off it.
- [ ] At least three of the seven 97.4 slices ship + pass in
      `just test-all` before this phase is considered "done"
      (rest can roll incrementally as priorities allow).
- [ ] Archived Phase 71 doc cross-links to this phase so the
      historical context is one click away.
- [ ] `book/src/user-guide/rmw-backends.md` "DDS — per-platform
      configuration profile" stays in sync; closed slices switch
      from "TODO" prose to "see `examples/<plat>/rust/dds/`" links.

## Notes

- **Order matters.** 97.1.cs gates every slice; ship it first, then
  pick one platform end-to-end as the template (recommend FreeRTOS —
  most mature lwIP support; MPS2-AN385 board crate already exists for
  zenoh examples).
- **Per-platform bring-up cost is non-trivial.** Each 97.4 slice is
  realistically 1–2 days of work end-to-end (cs / linker / heap /
  Kconfig / fixture / first-boot debugging). The matrix takes time;
  closing one platform per week is a reasonable cadence.
- **No new traits or APIs.** Every piece consumes infrastructure
  Phase 71 already shipped. This phase is pure assembly.
- **Coordinate with Phase 64.2** (embedded transport tuning guide):
  the per-RTOS heap / lwIP / Kconfig knobs documented here also
  belong in Phase 64.2's narrative, once it lands.
