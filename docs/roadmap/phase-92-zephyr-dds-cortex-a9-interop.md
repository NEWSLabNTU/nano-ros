# Phase 92: Zephyr DDS talker↔listener interop on qemu_cortex_a9

**Goal**: Land a real talker→listener DDS pubsub interop test for the Zephyr `nros-rmw-dds` path that exercises Zephyr's native IP stack (the code path 95% of production Zephyr DDS deployments will run), without requiring `sudo` host setup or a vendored Zephyr SDK patch.

**Status**: In Progress
**Priority**: Medium
**Depends on**: Phase 71.8 (cooperative DDS runtime + boot smoke tests landed)

## Overview

### Why this exists

Phase 71.8 landed the cooperative `NrosPlatformRuntime<ZephyrPlatform>` + `NrosUdpTransportFactory` plus the multicast primitives (`mcast_listen`/`mcast_read`/`mcast_send`). Boot smoke tests pass (`test_zephyr_dds_rust_{talker,listener}_boots`). What's missing is a real interop test that proves talker → listener pubsub works end-to-end on Zephyr.

Investigating that test surfaced two structural blockers on `native_sim`:

1. **NSOS doesn't forward IPv4 multicast to the host kernel.** Zephyr's `nsos_adapt_setsockopt` only translates `SOL_SOCKET` / `IPPROTO_TCP` / `IPPROTO_IPV6` options. `IP_ADD_MEMBERSHIP` returns `EOPNOTSUPP`, so SPDP discovery silently fails and the listener never sees the talker. Fixing this requires patching three upstream Zephyr files — workspace-side, not our tree.
2. **TAP (`eth_native_posix`) needs `sudo` zeth-bridge setup.** Phase 81's archived doc records the multi-instance contention (`Cannot create zeth0`) plus `pthread_create(NULL attr)` bug; the latter was fixed via `nros_zephyr_task_create`, but the per-instance TAP names + host bridge still need root.

The third option — `qemu_cortex_a9` with QEMU's `-netdev socket,mcast=…` — sidesteps both: 512 MB SRAM (comfortable for dust-dds's ~4 MB heap requirement), Xilinx GEM ethernet driver in Zephyr, full Zephyr IP stack with native IGMP, and QEMU's mcast netdev creates a virtual L2 segment by joining a host-local UDP multicast group (no privileges, no TAP). Two QEMU instances joined to the same mcast group exchange Ethernet frames as if on the same wire.

This is also the **same code-path topology that real Zephyr DDS deployments use**: native IP stack on a real Ethernet PHY (vs. NSOS-on-native-sim, which mirrors the niche offload-modem topology and is irrelevant to standard DDS-on-Zephyr).

### Architecture

```
  ┌────────── host loopback ──────────┐
  │   UDP mcast group 230.0.0.1:N     │
  └────┬─────────────────────────┬────┘
       │                         │
  ┌────▼─────┐               ┌───▼──────┐
  │ qemu A9  │               │ qemu A9  │
  │ ┌──────┐ │   virtual L2  │ ┌──────┐ │
  │ │ DDS  │ │    via mcast  │ │ DDS  │ │
  │ │talker│ │◄──────────────►│listner│ │
  │ └──┬───┘ │               │ └──┬───┘ │
  │ Zephyr  │               │ Zephyr   │
  │ IP+IGMP │               │ IP+IGMP  │
  │ GEM drv │               │ GEM drv  │
  └────────┘                └──────────┘
```

`-netdev socket,id=net0,mcast=230.0.0.1:N` on each QEMU process joins the same host mcast group. QEMU forwards every frame the guest sends to that group; every frame received from the group is delivered to the guest. The two guests see each other as L2 neighbours. Zephyr's GEM driver sees a normal Ethernet link, and SPDP (RTPS multicast on `239.255.0.1:7400`) flows over the virtual segment.

## Work Items

- [x] 92.1 — Validate Cortex-A9 Rust toolchain
       Patched `modules/lang/rust/CMakeLists.txt` and `modules/lang/rust/Kconfig`
       upstream to add Cortex-A9/A7 cases (`armv7a-none-eabi[hf]`,
       `RUST_SUPPORTED`). Installed both targets via `rustup target add`.
       `dust_dds`, `nros-rmw-dds`, `nros-platform-zephyr`, and `nros`
       compile clean for `armv7a-none-eabi`. zephyr-lang-rust's
       `samples/philosophers` boots cleanly on `qemu_cortex_a9`.
       Documented patch under `scripts/zephyr/cortex-a9-rust-patch.sh`
       (TODO).
       Pick the right triple (`armv7a-none-eabi` vs `armv7a-none-eabihf`),
       install via rustup, confirm `dust_dds` + `nros-rmw-dds` +
       `nros-platform-zephyr` build clean for it. Risk: medium — most
       likely failure mode is x86-isms in 32-bit-pointer-size code or
       the C shim's `socklen_t = usize` assumption.

- [x] 92.2 — `qemu_cortex_a9` board overlay
       Add `boards/qemu_cortex_a9.conf` to `dds/talker` and
       `dds/listener` with `CONFIG_NET_IPV4_IGMP=y`,
       `CONFIG_ETH_XLNX_GEM=y`, distinct static IPs
       (192.0.2.1 / 192.0.2.2), `CONFIG_NET_IF_MCAST_IPV4_ADDR_COUNT=4`,
       and dust-dds-sized heap.

- [ ] 92.3 — `ZephyrProcess::CortexA9` launch variant
       Add CortexA9 to `ZephyrPlatform` in `nros-tests/src/zephyr.rs`.
       Wire `qemu-system-arm -machine arm-generic-fdt-7series -dtb …
       -netdev socket,id=net0,mcast=230.0.0.<N>:<port> -net nic …`.
       Pick a per-test (mcast-group, port) tuple keyed off the test
       binary's PID so concurrent runs of different tests don't bleed.

- [~] 92.4 — Build talker + listener for qemu_cortex_a9
       `west build -b qemu_cortex_a9` succeeds for both binaries
       (release profile, ~14 MB RAM, 0 errors). Boot banner does NOT
       appear when launched via `qemu-system-xilinx-aarch64` —
       suggests pre-kernel-init crash. Comparison points:
       * `zephyr-lang-rust/samples/philosophers` (no networking,
         no nros) boots clean.
       * Our talker / listener with the prj.conf as shipped (POSIX_API,
         NETWORKING, nros + nros-rmw-dds) goes silent.
       Bisecting which prj.conf option triggers the silent crash is
       the unresolved part. Likely candidates: `CONFIG_NETWORKING=y`
       triggering Xilinx GEM init that hits a Zephyr/QEMU edge case;
       `CONFIG_POSIX_API=y` interacting with the ARMv7-A SVC stack;
       or some interaction between the `arm-zephyr-eabi` GCC and
       Rust's `armv7a-none-eabi` link layout.

- [ ] 92.5 — Talker↔listener interop validation
       Two QEMU instances on the same mcast group; listener's stdout
       contains `Received: 0` (and ideally `Received: 5` within
       30 s of sim time). Debug SPDP / SEDP / data-path issues.

- [ ] 92.6 — Nextest interop test + serial group
       `test_zephyr_dds_rust_talker_to_listener_e2e` checked into
       `nros-tests/tests/zephyr.rs`. Add a `qemu-zephyr-dds` nextest
       group with `max-threads = 1` so concurrent test runs don't
       fight over the mcast group/port pair.

## Acceptance Criteria

- [ ] `cargo test -p nros-tests --test zephyr test_zephyr_dds_rust_talker_to_listener_e2e` passes locally without `sudo` and without any Zephyr SDK patches.
- [ ] Phase 71.8's roadmap entry can flip from `[~]` to `[x]`.
- [ ] No regressions in the existing 27 Zephyr E2E tests (`just zephyr test`).
- [ ] The new test runs under a max-threads=1 nextest group so it can coexist with the rest of the Zephyr suite without mcast-group collisions.

## Notes

### Why qemu_cortex_a9 specifically (vs other QEMU boards)

QEMU board RAM survey for dust-dds (~4 MB heap minimum):

| Board | SRAM | Notes |
|---|---|---|
| `qemu_cortex_m3` | 64 KB | Way too small. Stellaris LM3S6965. |
| `qemu_x86` / `qemu_x86_64` | 1 MB | Too small. |
| **`qemu_cortex_a9`** | **512 MB** | Zynq-7000 emulation, Xilinx GEM ethernet. ✓ |
| `qemu_cortex_a53` | 128 MB | AArch64 Cortex-A53. Also fits but adds a 64-bit Rust target. |
| `kvm_arm64` | (host RAM) | Requires KVM on host — not a CI-friendly default. |

`qemu_cortex_a9` is the smallest-friction option that fits dust-dds. AArch64 (`qemu_cortex_a53`) is a fine alternative if the ARMv7-A Rust target turns out to be flaky.

### Why mcast netdev (vs TAP, vs slirp user-mode networking)

| Option | Sudo? | Multicast? | Per-instance isolation |
|---|---|---|---|
| `-netdev tap` | yes | yes (host bridges) | needs per-test TAPs + bridge |
| `-netdev user` (slirp) | no | **no — slirp drops mcast** | per-instance NAT |
| **`-netdev socket,mcast`** | **no** | **yes** | per-test (group, port) tuple |

`-netdev user` is the obvious "no sudo" choice but slirp's NAT engine doesn't forward multicast — would silently break SPDP discovery the same way NSOS does. The mcast-socket netdev is the only no-`sudo` option that preserves L2 multicast.

### Per-instance IP addresses

QEMU mcast netdev gives both guests an L2 broadcast domain but doesn't auto-assign IPs. Each guest needs its own static IPv4 in the `prj.conf`'s board overlay. We'll use 192.0.2.1 (talker) and 192.0.2.2 (listener) — consistent with the Zephyr `qemu_cortex_m3` socket samples.

### Mcast group / port collision avoidance

Multiple test runs on the same host could collide if they use the same `(mcast_group, port)`. Plan: pick the port deterministically from a hash of the test binary's process group ID at launch time, or tie it to the existing per-platform port allocator in `nros_tests::platform`. The mcast group can stay fixed (`230.0.0.1`); the port is what scopes the virtual L2 segment.

### What this does NOT cover

- **Real hardware (Zynq, STM32-Eth, ESP32-MAC) DDS deployments** — same code path, but actual hardware bring-up is its own integration cost. Phase 92's test gives high confidence that the cooperative DDS runtime + RTPS bind sequence works against a real Zephyr IP stack; physical-board validation is a separate exercise.
- **Cross-vendor RTPS interop** — Phase 71.9 will exercise CycloneDDS / Fast-DDS against `nros-rmw-dds` once the in-tree pubsub test is green.
- **Performance / latency / throughput numbers** — Phase 92 is correctness-only; numbers belong in a follow-up bench task.

### Why not ship the Phase 71.8 doc with the NSOS patch as the recommended path

Two reasons:
1. NSOS-on-native-sim mirrors the offload-modem topology, which is the *minority* of real DDS-on-Zephyr deployments. Native IP stack is what production runs — we want our regression test on that path.
2. Patching upstream Zephyr puts every contributor on a different SDK from the one `west update` pinned. The QEMU mcast path lives entirely in nano-ros and doesn't drift.
