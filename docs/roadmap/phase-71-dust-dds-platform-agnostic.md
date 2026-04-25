# Phase 71 — DDS Backend on `nros-platform` Capability Traits

**Goal**: Bring `nros-rmw-dds` (dust-dds fork) to parity with
`nros-rmw-zenoh` and `nros-rmw-xrce` as a third shipping RMW backend —
native POSIX, Zephyr, NuttX, ThreadX-Linux, and bare-metal (smoltcp) —
driven by the unified `nros-platform-api` capability traits that Phase
84.F4 froze. No new C dependencies, no forked runtime, no alloc-free
fantasy.

**Status**: Partially landed. `nros-rmw-dds` ships a std-only POSIX path
today (`packages/dds/nros-rmw-dds/`); the fork (`packages/dds/dust-dds/`)
already compiles `no_std + alloc` via its own `nostd_test_project`.
What's missing is the transport + runtime adapter layer that lets the
backend open a session on every nros platform.

**Priority**: Medium. Zenoh and XRCE cover the embedded footprint;
DDS/RTPS is the interop story for ROS 2 stacks that use
`rmw_cyclonedds_cpp` / `rmw_fastrtps_cpp` without a zenoh router in
the middle. Dust-dds's RTPS implementation is already wire-compatible
with CycloneDDS and Fast-DDS — see
`packages/dds/dust-dds/interoperability_tests/{cyclone_dds,fast_dds}/`
which spin both implementations against each other in CI.

**Depends on**: Phase 70 (DDS RMW backend scaffolding — done),
Phase 84.F4 (platform trait contract — done), Phase 80 (unified
network interface — done for TCP/UDP via `PlatformTcp` /
`PlatformUdp` / `PlatformUdpMulticast`).

## Overview

### The design has already shifted under this phase doc

The original Phase 71 draft (pre-Phase 84.F4) proposed building a
standalone `NrosRuntime` struct inside dust-dds plus a bespoke
`dds-smoltcp` crate. That framing is obsolete:

* **Platform services** now live on `nros-platform-api`
  (`PlatformClock`, `PlatformTime`, `PlatformSleep`, `PlatformThreading`,
  `PlatformAlloc`, `PlatformUdp`, `PlatformUdpMulticast`,
  `PlatformNetworkPoll`). Every platform crate (`nros-platform-posix`,
  `-zephyr`, `-nuttx`, `-freertos`, `-threadx`) implements whichever
  subset applies. A DDS-specific runtime would duplicate every one
  of them.
* **Smoltcp infra** already lives in `packages/drivers/nros-smoltcp`
  (`SmoltcpBridge` + `NetworkState<D>`). Zenoh-pico consumes it via
  `zpico-platform-shim`; a DDS-over-smoltcp implementation would go
  through the same layer.
* **Dust-dds itself** already has a pluggable `DdsRuntime` trait and
  a `nostd_test_project/` that validates `no_std + alloc` builds; the
  real blockers are (a) the hardcoded `std_runtime::executor::block_on`
  in the sync API and (b) the UDP transport that spawns three blocking
  OS threads.

So the shape of Phase 71 collapses to: **adapt dust-dds's existing
pluggability points to `nros-platform-api`**, not reinvent a runtime.

### Current Architecture (what ships today)

```
┌──────────────────────────────────────────────────────┐
│ nros-rmw-dds  (RMW backend shim)                     │
│   std-only path:                                     │
│     DomainParticipantFactory::get_instance()         │
│     + dust_dds::rtps_udp_transport (3 OS threads)    │
│   #[cfg(not(feature = "std"))] → ConnectionFailed    │
└──────────────────────────────────────────────────────┘
                         │
┌──────────────────────────────────────────────────────┐
│ dust-dds fork (packages/dds/dust-dds/)               │
│   dcps/ dds_async/ rtps/ xtypes/                     │
│     — generic over R: DdsRuntime                     │
│     — `#![cfg_attr(not(feature = "std"), no_std)]`   │
│     — `extern crate alloc;`                          │
│     — CI-tested no_std + alloc build                 │
│   rtps_udp_transport/                                │
│     — hardcoded to socket2 + 3x std::thread         │
│   sync API (dds/)                                    │
│     — imports std_runtime::executor::block_on       │
│     — `thread::park`/`unpark` under the hood         │
└──────────────────────────────────────────────────────┘
```

### Target Architecture (post-phase-71)

```
┌──────────────────────────────────────────────────────┐
│ nros-rmw-dds                                         │
│   feature matrix mirrors nros-rmw-zenoh:             │
│     default = []                                     │
│     std / platform-posix / platform-zephyr /         │
│     platform-nuttx / platform-freertos /             │
│     platform-threadx / platform-bare-metal           │
└──────────────────────────────────────────────────────┘
                          │
┌──────────────────────────────────────────────────────┐
│ nros-rmw-dds::runtime (new, thin — ~200 lines)       │
│   struct NrosPlatformRuntime<P>;                     │
│   impl<P> DdsRuntime for NrosPlatformRuntime<P>      │
│     where P: PlatformClock + PlatformSleep +         │
│              PlatformThreading + PlatformAlloc { … } │
│   fn clock() → <P as PlatformClock>::clock_ms()      │
│   fn delay() → <P as PlatformSleep>::sleep_ms()      │
│   fn block_on() → cooperative spin on `spin_once()`  │
│                   (re-uses nros_node::Executor path) │
└──────────────────────────────────────────────────────┘
                          │
┌──────────────────────────────────────────────────────┐
│ nros-rmw-dds::transport (new — replaces              │
│   dust_dds::rtps_udp_transport on platforms other    │
│   than `platform-posix-with-std`)                    │
│                                                      │
│   platform-posix / zephyr / nuttx / threadx:         │
│     impl TransportParticipantFactory on              │
│     <P as PlatformUdp> + <P as PlatformUdpMulticast>│
│     (SPDP multicast + data unicast).                 │
│                                                      │
│   platform-bare-metal:                               │
│     same trait dispatch — the smoltcp board crate    │
│     wires PlatformUdp / PlatformUdpMulticast via     │
│     SmoltcpBridge, same as zpico-smoltcp does today. │
└──────────────────────────────────────────────────────┘
```

Notably: **no new `dds-smoltcp` crate**. The bare-metal path reuses the
same `SmoltcpBridge` / `NetworkState<D>` infra that zenoh-pico consumes.
The only DDS-specific adapter is the `TransportParticipantFactory` shim
that turns `<P as PlatformUdp>::send(...)` into an RTPS datagram pusher.

### Design Principles (unchanged from the original doc, restated)

1. **Library defines interfaces, platform implements them.** Apply this
   both to dust-dds core (already true via `DdsRuntime`) and to the
   transport (today false — to be fixed by 71.2 + 71.3).
2. **Non-blocking I/O**. Transport `recv` returns `Option<...>` /
   `TryRecv`; the caller drives the loop. Matches the
   `zpico_spin_once()` / `Executor::spin_once()` pattern the rest of
   the project uses.
3. **Cooperative execution**. On every platform the RTPS reader
   background work happens inside `nros_node::Executor::spin_once()`
   via a new arena entry (see 71.4). No background threads even on
   POSIX — this is what makes the model identical across platforms.
4. **Heap is mandatory; alloc-free is out of scope.** Dust-dds is
   `no_std + alloc`, not `no_std` solo. Users must enable an allocator
   (embedded-alloc / linked-list-allocator / a platform heap) on
   bare-metal. The XRCE / zenoh-pico targets that require alloc-free
   should continue to use those backends.

## Platform Service Mapping

Dust-dds's `DdsRuntime` trait expects `clock`, `timer`, `spawn`, and
(after 71.1) `block_on`. The mapping to `nros-platform-api` is
one-to-one:

| `DdsRuntime` method     | Backed by                                     | Platforms |
|-------------------------|-----------------------------------------------|-----------|
| `clock()`               | `<P as PlatformClock>::clock_ms()`            | all |
| `timer.delay(Duration)` | `<P as PlatformSleep>::sleep_ms()` (blocking) **or** cooperative deadline-checked future polled by `spin_once()` | all |
| `spawner.spawn(fut)`    | static task pool in `nros-rmw-dds::runtime` polled by `spin_once()` | all |
| `block_on(fut)` (71.1)  | loop calling `nros_node::Executor::spin_once()` until the future resolves (same shape as `Promise::wait`) | all |

Network I/O sits on:

| dust-dds call          | Backed by                                        |
|------------------------|--------------------------------------------------|
| factory::create_participant | creates two sockets via `<P as PlatformUdp>::open()` + `<P as PlatformUdpMulticast>::mcast_listen()` |
| `WriteMessage::write_message` | `<P as PlatformUdp>::send()` (unicast) or `<P as PlatformUdpMulticast>::mcast_send()` (SPDP announce) |
| recv drive             | polled from `spin_once()` via `<P as PlatformUdp>::read()` with a short timeout set by `set_recv_timeout` |

The critical observation: **every primitive we need is already on the
`nros-platform-api` surface**, because the zenoh-pico shim
(`zpico-platform-shim`) needed the exact same primitives for the same
reason. Phase 71 is a much smaller delta than the pre-F4 draft
suggested.

## Work Items

- [x] 71.1 — `block_on` on `NrosPlatformRuntime` (no fork patch needed)
- [x] 71.2 — Non-blocking UDP transport (`NrosUdpTransportFactory`,
       Path B): bind, SPDP multicast join, recv loops spawned to runtime
- [x] 71.3 — `NrosPlatformRuntime<P>` adapter (`nros-rmw-dds::runtime`)
- [x] 71.4 — `drive_io()` drives the runtime + `Rmw::open` no_std path
- [x] 71.5 — Feature-gated backend selection in `nros-rmw-dds`
- [x] 71.4.b — Port `DdsPublisher` / `DdsSubscriber` / `DdsService*` from
       sync to async dust-dds API + block_on wrap, unblocking actual no_std
       end-to-end pubsub
- [ ] 71.6 — Board-crate `#[global_allocator]` support (off by default)
- [ ] 71.7 — Bare-metal QEMU DDS talker/listener example + nextest suite
- [~] 71.8 — Zephyr DDS talker/listener example + nextest suite
       (scaffolding + clean Kconfig/CMake wiring + native_sim build green;
       runtime currently hangs in `block_on(create_participant)` —
       same hang reproduces on POSIX, see `cooperative_pubsub_posix.rs`
       which is `#[ignore]` until the hang is root-caused)
- [ ] 71.9 — (Optional) CycloneDDS / Fast-DDS interop test in nros-tests
- [ ] 71.10 — (Optional) Upstream non-blocking transport to dust-dds

### Sub-phase 71.20–71.27 — Complete `PlatformUdp` for dust-dds across every platform

While auditing the Path B transport against POSIX's
`udp_open` we discovered that the existing `PlatformUdp` trait was
designed entirely around zenoh-pico's outbound-only UDP usage:
`open(sock, endpoint, timeout)` creates a socket fd from the endpoint
metadata but **never calls `bind(2)`** — only the multicast path
(`PlatformUdpMulticast::mcast_listen`) does an explicit bind. That's
fine for zenoh-pico (every `z_get` opens a fresh ephemeral port) but
breaks DDS, which needs deterministic local-port binds for
SPDP/SEDP discovery and for peers to know where to send unicast
samples. The `bind_unicast` helper in `transport_nros.rs` calls
`PlatformUdp::open` today and gets a non-bound socket; recv loops
attached to those sockets receive nothing.

Closing the gap touches every platform implementation. The work is
split into independently-landable items below; once they're done,
71.4.b's port to the async API gives us an end-to-end no_std pubsub
on every nros platform.

- [x] 71.20 — `PlatformUdp::listen` (bind) used as the bind primitive — already
       on the trait surface as a default `-1` stub
- [x] 71.21 — Per-platform `PlatformUdp::listen` implementations (×6)
- [ ] 71.22 — Replace `[u8; 64]` opaque buffers with size-probed
       `nros-rmw-dds`-owned types
- [ ] 71.23 — Per-platform SDK / Kconfig profile for DDS
- [ ] 71.24 — Host-side `PlatformUdp` validation suite
       (POSIX loopback unit tests)
- [ ] 71.25 — Per-platform QEMU smoke binary
       (bind → recvfrom → assert)
- [ ] 71.26 — Bare-metal smoltcp multicast (IGMP) audit
- [ ] 71.27 — End-to-end DDS pubsub QEMU E2E test, one per platform

#### 71.20 / 71.21 — bind primitive — **Landed**

Discovered while implementing that `PlatformUdp::listen` was already
on the trait surface as a default `-1` stub (originally added for
zenoh-pico's UDP server-mode locators). It has the right signature
for our needs — `(sock, endpoint, timeout_ms) -> i8`. So no new
trait method was needed; just the per-platform overrides.

71.21 implementations (`udp_listen` helper + trait override):

* `nros-platform-posix` — `libc::socket` + `SO_REUSEADDR` +
  `SO_RCVTIMEO` + `bind(2)`. Verified by the new
  `bind_unicast_then_send_then_recv_roundtrip_posix` unit test which
  binds on port 47411 and round-trips a datagram via loopback.
* `nros-platform-zephyr` — same shape via the Zephyr POSIX socket
  shim (`c::socket` + `c::setsockopt(SO_REUSEADDR/SO_RCVTIMEO)` +
  `c::bind`).
* `nros-platform-freertos` — lwIP `lwip_socket` +
  `lwip_setsockopt` + `lwip_bind`.
* `nros-platform-nuttx` — NuttX BSD socket layer (`socket`,
  `setsockopt`, `bind`).
* `nros-platform-threadx` — NetX BSD layer (`nx_bsd_socket`,
  `nx_bsd_setsockopt`, `nx_bsd_bind`). Replaces the connect-style
  `udp_open` for inbound use.
* `nros-smoltcp::define_smoltcp_platform!` — adds a new
  `SmoltcpBridge::udp_set_local_port(handle, port)` primitive that
  records `entry.local_port` on the bridge's UDP socket table; the
  next `do_poll()` sees `socket.is_open() == false` and calls
  `socket.bind(port)`. The `listen()` impl in the macro reserves a
  handle via `udp_open()` then immediately calls
  `udp_set_local_port(handle, ep._port)` so the bind happens on the
  RTPS PSM port rather than an ephemeral one.

`bind_unicast` in `nros-rmw-dds::transport_nros` now calls
`<P as PlatformUdp>::listen(sock, ep, 0)` instead of `open` — the
DDS recv loops finally bind to deterministic ports on every
platform.

Verification (host):
* `cargo check` clean for `nros-platform-{posix,zephyr,freertos,
  nuttx,threadx}`.
* `cargo check --target thumbv7m-none-eabi --manifest-path
  packages/boards/nros-platform-mps2-an385/Cargo.toml` clean.
* `cargo test -p nros-rmw-dds --features platform-posix --lib`:
  10/10 pass (was 9; the new POSIX bind round-trip test is the
  10th).
* `cargo nextest run -p nros-tests --test dds_api`: 5/5 pass —
  std + posix path unaffected.

QEMU + cross-compile per-platform validation is 71.25.

**Files**:
- `packages/core/nros-platform-posix/src/net.rs`
- `packages/core/nros-platform-zephyr/src/net.rs`
- `packages/core/nros-platform-freertos/src/net.rs`
- `packages/core/nros-platform-nuttx/src/net.rs`
- `packages/core/nros-platform-threadx/src/net.rs`
- `packages/drivers/nros-smoltcp/src/bridge.rs`
  (new `udp_set_local_port`)
- `packages/drivers/nros-smoltcp/src/platform_macro.rs`
  (`listen()` override on the smoltcp platform)
- `packages/dds/nros-rmw-dds/src/transport_nros.rs`
  (`bind_unicast` calls `listen` + new round-trip unit test)

#### 71.20 — Add `PlatformUdp::bind` — historical (kept for context)

The original draft of this item proposed a new trait method.
Re-reading the trait surface during implementation showed
`PlatformUdp::listen` already had the right signature, so no
trait extension was needed.

The trait gains one method:

```rust
pub trait PlatformUdp {
    // ... existing methods ...

    /// Bind a fresh UDP socket to `endpoint` for receiving.
    ///
    /// `endpoint` is the LOCAL address to bind (typically
    /// `0.0.0.0:<port>` rendered via `create_endpoint`). After a
    /// successful return, `read()` and `read_exact()` deliver
    /// datagrams sent to that port; `send()` chooses the source port
    /// to match.
    ///
    /// Distinct from `open()` which only creates a socket fd from
    /// the endpoint metadata without binding — that one stays in
    /// place for outbound zenoh-pico usage. Returns 0 on success,
    /// negative on failure.
    fn bind(sock: *mut c_void, endpoint: *const c_void, timeout_ms: u32) -> i8;
}
```

Default impl returning `-1` so platforms that haven't migrated yet
fail loudly rather than silently no-op:

```rust
fn bind(_sock: *mut c_void, _endpoint: *const c_void, _timeout_ms: u32) -> i8 {
    -1
}
```

**Files**:
- `packages/core/nros-platform-api/src/lib.rs` — new method on
  `PlatformUdp` with default `-1`.

#### 71.21 — Per-platform implementations of `PlatformUdp::bind`

Six impls; each one is small (~10 lines) but has to thread through
the platform's actual networking stack:

| Platform | File | Sketch |
|---|---|---|
| POSIX | `nros-platform-posix/src/net.rs` | `libc::socket` + `libc::bind` + `SO_REUSEADDR` + `SO_RCVTIMEO`; pattern in existing `tcp_listen` |
| Zephyr | `nros-platform-zephyr/src/net.rs` | `zsock_socket` + `zsock_bind`; needs `CONFIG_POSIX_API=y` from 71.23 |
| FreeRTOS+lwIP | `nros-platform-freertos/src/net.rs` | lwIP `lwip_bind`; needs `LWIP_SO_RCVTIMEO=1` |
| NuttX | `nros-platform-nuttx/src/net.rs` | `socket` + `bind` via NuttX libc |
| ThreadX+NetX | `nros-platform-threadx/src/net.rs` | NetX BSD `bind` from `<nxd_bsd.h>` |
| Bare-metal smoltcp | `nros-smoltcp/src/platform_macro.rs` | `SmoltcpBridge::create_udp_socket` + bind via socket's `bind(IpEndpoint)`; the bridge already exposes the primitive |

Each of these has to round-trip a localhost loopback test (71.24)
before being marked done.

#### 71.22 — Size-probed opaque types

Today `transport_nros.rs` uses `[u8; 64]` for both `OpaqueSocket`
and `OpaqueEndpoint` — comfortably over the largest observed
platform (POSIX 16 bytes, Zephyr 8 bytes, smoltcp 2 bytes), but
wasteful. `zpico-platform-shim/build.rs` already has a working
`cc::Build`-based size probe driven by the platform's
`_z_sys_net_socket_t` / `_z_sys_net_endpoint_t`.

Two choices:
- **A** — `nros-rmw-dds` re-runs that build script (copy
  `zpico-platform-shim/build.rs` into `nros-rmw-dds/build.rs`).
  Adds ~150 LOC of build script per crate.
- **B** — Factor the size-probe types out of `zpico-platform-shim`
  into a shared `nros-net-shim` crate that both `zpico` and
  `nros-rmw-dds` depend on. Cleaner long-term but requires
  rearranging `zpico-platform-shim`'s `pub`-visibility surface.

Recommended: **B**. The `ZSysNetSocket` / `ZSysNetEndpoint` names
become `NetSocket` / `NetEndpoint` after the move; `zpico` keeps its
existing names as aliases.

#### 71.23 — Per-platform SDK / Kconfig profile for DDS

DDS over UDP multicast needs IGMP and adequate RX queue depth on
every RTOS. Concrete deltas per platform:

| Platform | Required configuration |
|---|---|
| POSIX | none |
| Zephyr | `CONFIG_POSIX_API=y`, `CONFIG_NET_IPV4_IGMP=y`, `CONFIG_NET_SOCKETS_POLL_MAX≥6`, `CONFIG_NET_PKT_{RX,TX}_COUNT≥32`, `CONFIG_NET_BUF_RX_COUNT≥256`, `CONFIG_NET_BUF_DATA_SIZE≥512` |
| FreeRTOS+lwIP | `LWIP_IGMP=1`, `LWIP_SO_RCVTIMEO=1`, `LWIP_BROADCAST=1`, `IP_REASSEMBLY=1` (RTPS fragments), `MEMP_NUM_NETBUF≥32` |
| NuttX | `CONFIG_NET_IGMP=y`, `CONFIG_NET_BROADCAST=y`, `CONFIG_NET_UDP_NRECVS≥4`, `CONFIG_NET_RECV_TIMEO=y` |
| ThreadX+NetX | `NX_ENABLE_IGMP_VERSION2`, NetX BSD layer init for SO_RCVTIMEO |
| Bare-metal smoltcp | smoltcp `MulticastConfig::Strict`; bridge config exposes `Interface::join_multicast_group(IpAddress::v4(239,255,0,1))` |

Each profile lives in the matching board crate's `prj.conf` /
`FreeRTOSConfig.h` / Kconfig fragment. Documented in
`book/src/user-guide/rmw-backends.md`.

#### 71.24 — Host-side `PlatformUdp` validation suite

POSIX-only unit-test crate that exercises the contract dust-dds
expects:

```rust
#[test]
fn bind_recvfrom_loopback() {
    let bound = bind_to(7411);          // <P as PlatformUdp>::bind
    let outbound = open_to(7411);
    send_to(outbound, b"hello", "127.0.0.1", 7411);
    set_recv_timeout(bound, 100);
    let buf = recv(bound);
    assert_eq!(buf, b"hello");
}
```

Lives in `packages/dds/nros-rmw-dds/tests/platform_udp_*.rs`. For
non-host platforms (Zephyr, FreeRTOS, etc.) this becomes a "compile
+ run in QEMU" test (71.25).

#### 71.25 — Per-platform QEMU smoke binary

For each cross-compile platform, ship a `tests/`-style binary
shaped like:

```
fn main() {
    let bound = <ConcretePlatform as PlatformUdp>::bind(...);
    let outbound = <ConcretePlatform as PlatformUdp>::open(...);
    send(outbound, "hello");
    let buf = recv(bound);
    assert_eq!(buf, "hello");
    println!("[OK] PlatformUdp roundtrip");
}
```

Run it under each platform's QEMU configuration (Zephyr native_sim,
qemu-arm-freertos, qemu-arm-nuttx, qemu-riscv64-threadx, MPS2-AN385
bare-metal, ESP32-QEMU). Pattern matches existing
`tests/qemu-*` scripts.

#### 71.26 — Bare-metal smoltcp multicast audit

smoltcp 0.x supports IGMP v1/v2 via `Interface::join_multicast_group`
and `MulticastConfig::Strict`. Three checks:

1. `nros-smoltcp::SmoltcpBridge` exposes a multicast-join API
   (currently it doesn't — only TCP / UDP unicast).
2. `define_smoltcp_platform!` macro's `PlatformUdpMulticast` impl
   wires `mcast_listen` through to the new bridge API.
3. End-to-end test on MPS2-AN385 + ESP32-QEMU: subscribe to
   `239.255.0.1:7400`, receive an SPDP packet from a host-side
   sender.

If smoltcp's IGMP support proves too restrictive (e.g. no
multicast-loopback), document the limitation and have the
bare-metal target fall back to unicast-only DDS over slirp (peers
send to `10.0.2.2:7411` directly).

#### 71.27 — End-to-end DDS pubsub QEMU E2E test, one per platform

Two QEMU instances (talker + listener) on each platform exchanging
RTPS data. Each platform gets its own nextest binary mirroring the
existing zenoh `rtos_e2e` shape (4-platform `#[rstest]` matrix).
Acceptance: a `RawCdrPayload` round-trips both ways within 30 s.

Runs in `just <platform> test` per the existing per-platform
nextest groups (Phase 89.1's per-platform parallelism).

**Foundation + transport + open path landed.** Items 71.1–71.5 give
us a complete chain: `NrosPlatformRuntime<P>` (clock + timer +
spawner + block_on), `NrosUdpTransportFactory<P>` (3 sockets bound
to the RTPS PSM 9.6.1.4 ports + SPDP multicast join + recv loops
spawned onto the runtime), `DdsSession::drive_io()` driving the
spawner from `Executor::spin_once()`, and `DdsRmw::open()`
constructing a `DomainParticipantAsync` on every nros platform.

**What's missing for an end-to-end no_std pubsub** has narrowed to
**71.20–71.27** — closing the `PlatformUdp` gap. The trait was
designed for zenoh-pico's outbound-only UDP and lacks `bind`;
the `bind_unicast` helper in `transport_nros.rs` calls
`PlatformUdp::open` today and gets a non-bound socket. 71.20
adds the trait method, 71.21 implements it on six platforms,
71.22–71.27 cover size-probed buffers / SDK config / per-platform
smoke tests / smoltcp multicast / E2E pubsub tests.

71.4.b is now landed — `DdsPublisher` / `DdsSubscriber` /
`DdsServiceServer` / `DdsServiceClient` all support both the
sync dust-dds API (`std + platform-posix`) and the async API
(every other platform via `nostd-runtime`). The
`Session::create_*` methods construct the right variant per
platform feature.

### 71.1 — `block_on` on `NrosPlatformRuntime` — **Landed** (no fork patch needed)

The original framing assumed the sync API had to be patched to make
`block_on` pluggable. Re-reading `dust-dds/dds/src/lib.rs` showed that
the entire sync API (`dds/`) and `std_runtime` module are already
`#[cfg(feature = "std")]`-gated:

```rust
#[cfg(feature = "std")]
mod dds;
#[cfg(feature = "std")]
pub use dds::*;
...
#[cfg(feature = "std")]
pub mod std_runtime;
```

— so on `no_std`, the sync API and its `std_runtime::executor::block_on`
call are simply not in the build. The no_std path already has to use
the async API (`dds_async::*`) anyway, which needs its own `block_on`
to collapse each async call back to a sync caller. That's what
`NrosPlatformRuntime::block_on()` (inherent method, not a trait method)
provides:

```rust
impl<P: PlatformClock + PlatformSleep + 'static> NrosPlatformRuntime<P> {
    pub fn block_on<T>(&self, future: impl Future<Output = T>) -> T {
        let waker = noop_waker();
        let mut cx  = Context::from_waker(&waker);
        let mut f   = core::pin::pin!(future);
        loop {
            if let Poll::Ready(v) = f.as_mut().poll(&mut cx) { return v; }
            self.spawner.drain_tasks();                 // drive background
            <P as PlatformSleep>::sleep_ms(1);          // yield to OS / RTOS
        }
    }
}
```

Key properties:
- No fork patch — every dust-dds file that imports
  `std_runtime::executor::block_on` stays exactly as-is (those files
  only get compiled in `std` builds anyway).
- Drives both the caller's future and the background spawner queue on
  every iteration, so spawned tasks (future RTPS receive loops,
  reliability timers) can make progress while `block_on` is waiting.
- Yields to the platform scheduler via `<P as PlatformSleep>::sleep_ms(1)`
  — no thread parking, no condvar. Matches the zpico side's
  cooperative driving semantics.

Covered by two new unit tests (`block_on_resolves_ready_future`,
`block_on_drives_spawned_side_task`). Both pass.

**Files**:
- `packages/dds/nros-rmw-dds/src/runtime.rs` — inherent method +
  2 tests. No fork changes.

### 71.2 — Non-blocking UDP transport — **Path B started**

Path B chosen (Path A would be std-only and require fork surgery for
the same end-state). The skeleton lives in
`packages/dds/nros-rmw-dds/src/transport_nros.rs`:

* `NrosUdpTransportFactory<P>` — implements
  `dust_dds::transport::TransportParticipantFactory`. Holds an
  `Arc<NrosPlatformRuntime<P>>` for spawning recv tasks and a
  configurable fragment size (defaults to 1344 to match dust-dds's
  stock builder). Generic over `P` so it picks up any platform that
  implements `PlatformUdp + PlatformUdpMulticast`.
* `NrosMessageWriter<P>` — implements `WriteMessage`. Holds one
  outbound socket protected by `spin::Mutex`. `write_message`
  iterates the locator list, formats each as a null-terminated IPv4
  endpoint string (`"a.b.c.d\0"` + `"port\0"`), calls
  `<P as PlatformUdp>::create_endpoint` + `send` + `free_endpoint`.
  Returns a Ready future immediately — the actual send is
  synchronous because `PlatformUdp::send` is non-blocking under the
  contract.
* `OpaqueSocket` / `OpaqueEndpoint` — `[u8; 64]` `repr(C, align(8))`
  buffers covering every shipped platform's `_z_sys_net_socket_t` /
  `_z_sys_net_endpoint_t` (largest currently observed: 16 bytes;
  smallest: 2 bytes on smoltcp). A follow-up commit can wire in
  `zpico-platform-shim`'s build-time size probe to make these exact
  rather than over-allocated.

**Landed in follow-up**: items 1–3 below all done in the second
71.2 commit.

1. ✅ RTPS port formulas (PSM 9.6.1.4) — `port_metatraffic_multicast`,
   `port_metatraffic_unicast`, `port_default_unicast`. Tested.
2. ✅ Three socket binds (`bind_unicast`, `bind_unicast` again,
   `bind_multicast` joining `239.255.0.1`). Locator lists populated
   for each successfully-bound socket.
3. ✅ Three async recv loops (`unicast_recv_loop`,
   `multicast_recv_loop`) spawned onto `runtime.spawner_handle()`.
   Each calls `set_recv_timeout(0)` once and loops
   `read()`/`mcast_read()` + `YieldOnce` to give the cooperative
   spawner control. Datagrams are wrapped as `Arc<[u8]>` and pushed
   through the dust-dds `MpscSender`.

**Still pending**:

4. Plumb the factory into `DdsRmw::open()` on `!std` so the no_std
   path actually constructs a participant — currently the factory
   is reachable only via direct construction in tests.

**Tests in this commit**: 9 unit tests pass:
* `locator_to_cstring_roundtrip`
* `factory_default_fragment_size_is_1344`
* `rtps_port_formulas_match_spec`
* `ipv4_locator_layout_matches_dust_dds`
* (5 runtime tests carried over)

End-to-end verification waits on item 4 above.

**Files**:
- `packages/dds/nros-rmw-dds/src/transport_nros.rs` (new, ~330 LOC)
- `packages/dds/nros-rmw-dds/src/lib.rs` — `pub mod transport_nros;`

The original Path A vs Path B note is kept below for reference, but
71.2 is now committed as Path B exclusively.

#### Path notes (historical — Path B chosen)

Two implementation paths considered:

**Path A — fork-patch `rtps_udp_transport`.** Replace the three
`std::thread::spawn` recv loops in
`dust-dds/dds/src/rtps_udp_transport/udp_transport.rs` with a
non-blocking driver. Smallest code change (the rest of the transport
— locator handling, multicast join, `MessageWriter::write_message` —
stays). But this path is `std`-only (uses `std::net::UdpSocket` +
`socket2::Socket`), so it only buys the cooperative-execution
property on POSIX; no_std still needs a second transport.

**Path B — new `nros-rmw-dds::transport_nros` module.** Implement
`dust_dds::transport::TransportParticipantFactory` from scratch on
top of `PlatformUdp` + `PlatformUdpMulticast`. Covers every nros
platform (POSIX, Zephyr, NuttX, FreeRTOS, ThreadX, bare-metal via
`SmoltcpBridge`). The catch is that `PlatformUdp`'s socket handle is
an opaque `*mut c_void` whose size is derived by `zpico-platform-shim`
from the platform's `_z_sys_net_socket_t` — we'd need a sibling
`dds-platform-shim` (size-probed at build time) or direct
`PlatformUdp` usage with a manual size reservation.

**Recommended split**:

1. **71.2.a** — Path A for native testing. ~150 LOC fork patch; lets
   `test_dds_talker_listener_communication` use the cooperative
   runtime end-to-end and unblocks 71.9 (cross-vendor interop test)
   without also needing an embedded path.

2. **71.2.b** — Path B for embedded. Lives in `nros-rmw-dds`, no
   fork change, reuses `SmoltcpBridge` on bare-metal. ~400 LOC.
   Unblocks 71.7 / 71.8.

3. **71.2.c** — Drop Path A in favour of Path B once Path B is stable
   on POSIX too (collapses the matrix to a single transport).

**Files (Path A)**:
- `packages/dds/dust-dds/dds/src/rtps_udp_transport/udp_transport.rs`

**Files (Path B)**:
- `packages/dds/nros-rmw-dds/src/transport_nros.rs` (new)
- `packages/dds/nros-rmw-dds/build.rs` (size-probe for the platform
  socket handle, mirrors `zpico-platform-shim/build.rs`)

### 71.3 — `NrosPlatformRuntime<P>` adapter — **Landed**

New module at `packages/dds/nros-rmw-dds/src/runtime.rs` implementing
dust-dds's `DdsRuntime` trait by dispatching through
`<P as PlatformClock>` and `<P as PlatformSleep>`. Three pieces:

- **`NrosClock<P>`** — `Clock::now()` calls `<P as PlatformClock>::clock_ms()`
  and converts to `dust_dds::infrastructure::time::Time` (seconds +
  nanoseconds). Zero-sized, `Clone + Send + Sync` via
  `PhantomData<fn() -> P>`.
- **`NrosTimer<P>` / `NrosSleep<P>`** — `Timer::delay()` returns a
  deadline-polled future. Each `poll()` compares the current platform
  clock against the stored deadline and, if not yet ready, calls
  `cx.waker().wake_by_ref()` to request immediate re-polling. Busy but
  O(N) per active delay; good enough for RTPS's handful of heartbeat /
  reliability timers. A follow-up (Phase 71.10 territory) can
  implement a TimerHeap like `std_runtime::timer::TimerHeap`.
- **`NrosSpawner`** — `Arc<Mutex<VecDeque<Pin<Box<dyn Future<..> + Send>>>>>`.
  `spawn()` pushes; `drain_tasks()` pops every pending task, polls
  once with a no-op waker, re-queues survivors. Intended to be called
  from Phase 71.4's arena hook.
- **`NrosPlatformRuntime<P>`** — wraps the spawner and implements
  `DdsRuntime` with `ClockHandle = NrosClock<P>`,
  `TimerHandle = NrosTimer<P>`, `SpawnerHandle = NrosSpawner`.
  Exposes `drive()` that Phase 71.4 will wire into
  `Executor::spin_once()`.

The `Mutex` type is swapped at compile time:
`std::sync::Mutex` under `feature = "std"`, `spin::Mutex` otherwise —
the file's top-level type alias `Mutex<T>` means the rest of the
module is `cfg`-free.

**Status**: clock + timer + spawner all verified by three unit tests
(`clock_returns_monotonic_time`, `spawner_runs_ready_future`,
`spawner_reschedules_pending_future`) — all 3/3 pass against
`ConcretePlatform = PosixPlatform`. The adapter is not yet wired into
`Rmw::open` — that's Phase 71.4's job (the arena hook needs to drive
`NrosPlatformRuntime::drive()` every `spin_once()`, and `Rmw::open`
needs to pick `NrosPlatformRuntime<ConcretePlatform>` instead of
`StdRuntime` when `std` is off).

**Files**:
- `packages/dds/nros-rmw-dds/src/runtime.rs` (new, ~280 lines)
- `packages/dds/nros-rmw-dds/src/lib.rs` — `pub mod runtime;` (feature-gated)
- `packages/dds/nros-rmw-dds/Cargo.toml` — `nros-platform` + `spin` deps

### 71.4 — `DdsSession::drive_io()` drives the runtime — **Skeleton landed**

`DdsSession` now owns an `Arc<NrosPlatformRuntime<ConcretePlatform>>`
on the `alloc + !std` path, and its `Session::drive_io()` impl calls
`runtime.drive()` each spin:

```rust
fn drive_io(&mut self, _timeout_ms: i32) -> Result<(), Self::Error> {
    #[cfg(all(feature = "alloc", not(feature = "std")))]
    { self.runtime.drive(); }
    Ok(())
}
```

The hook is wired through the existing `nros_rmw::Session` trait that
`nros_node::Executor` already calls once per `spin_once()` — no new
`ArenaEntryKind` is needed. On `std + platform-posix` the stock
dust-dds transport keeps its OS threads; `drive_io()` stays a no-op
there.

**Currently effective**: no. Background tasks (RTPS recv loops,
reliability timers) are only spawned into the runtime once 71.2's
transport lands. Until then the queue is permanently empty and
`drive()` does zero work. This commit is the wire-up that 71.2 will
activate.

**Files**:
- `packages/dds/nros-rmw-dds/src/session.rs` — `runtime` field +
  `new_nostd()` constructor + `drive_io()` body.

### 71.5 — Feature-gated backend selection in `nros-rmw-dds` — **Cargo.toml landed; runtime selection pending 71.4**

The `Cargo.toml` feature matrix now mirrors `nros-rmw-zenoh`:

```toml
[features]
default = []
std   = ["alloc", "nros-rmw/std", "dust_dds/std", "dust_dds/rtps_udp_transport"]
alloc = []
platform-posix    = ["std", "alloc", "nros-platform/platform-posix"]
platform-zephyr   = ["alloc", "nros-platform/platform-zephyr"]
platform-freertos = ["alloc", "nros-platform/platform-freertos"]
platform-nuttx    = ["alloc", "nros-platform/platform-nuttx"]
platform-threadx  = ["alloc", "nros-platform/platform-threadx"]
```

`platform-bare-metal` is deliberately omitted until Phase 71.6 ships an
opt-in `#[global_allocator]` on the affected board crates — without it
the feature would have no heap and `dust_dds` wouldn't link.

The `Rmw::open()` side of the selection (`StdRuntime` vs
`NrosPlatformRuntime<ConcretePlatform>` + nros-rmw-dds UDP transport)
is blocked on Phase 71.1 (sync API still imports
`std_runtime::executor::block_on` directly), Phase 71.2 (transport is
hardcoded to three OS threads), and Phase 71.4 (arena hook driving
`NrosPlatformRuntime::drive()`). The structural `cfg` branches in
`transport.rs` remain `std`-only for now.

**Files**:
- `packages/dds/nros-rmw-dds/Cargo.toml` — Cargo matrix + `spin` dep +
  `nros-platform` dep.
- `packages/dds/nros-rmw-dds/src/transport.rs` — unchanged pending 71.4.

### 71.6 — Board-crate `#[global_allocator]` support (opt-in)

Bare-metal boards that consume `nros-rmw-dds` need a `#[global_allocator]`.
Add an `alloc` feature to the relevant board crates (`nros-mps2-an385`,
`nros-stm32f4`, `nros-esp32-qemu`) that pulls `embedded-alloc` and
sets up a `LinkedListAllocator` backed by a static `[u8; 65_536]` byte
array in SRAM. Default is off so zenoh-pico / XRCE bare-metal targets
don't pay the ~4 KiB heap metadata cost.

Document in `book/src/getting-started/bare-metal.md` that enabling
`nros-rmw-dds` on a bare-metal board requires the `alloc` feature.

**Files**:
- `packages/boards/nros-mps2-an385/src/alloc.rs` (new, feature-gated)
- `packages/boards/nros-stm32f4/src/alloc.rs` (new, feature-gated)
- `packages/boards/nros-esp32-qemu/src/alloc.rs` (new, feature-gated)

### 71.7 — Bare-metal QEMU DDS talker/listener

Two examples plus a nextest binary:

- `examples/qemu-arm-baremetal/rust/dds/talker/`
- `examples/qemu-arm-baremetal/rust/dds/listener/`
- `packages/testing/nros-tests/tests/dds_qemu.rs`

Uses the same `SmoltcpBridge` + slirp-NAT networking that
`test_qemu_rtic_pubsub_e2e` already uses (port 7447 — DDS default).

### 71.8 — Zephyr DDS talker/listener

Native_sim first (matches zenoh's Phase 81 NSOS path — host loopback,
no TAP). Hardware board second. Example pair + nextest binary.

- `examples/zephyr/rust/dds/talker/`
- `examples/zephyr/rust/dds/listener/`

### 71.9 — (Optional) CycloneDDS / Fast-DDS cross-vendor interop test

Dust-dds's fork already ships CI-exercised interop tests against
CycloneDDS and Fast-DDS
(`packages/dds/dust-dds/interoperability_tests/`). Lift one pair into
`nros-tests` — a native `nros-rmw-dds` publisher talking to a
`CycloneDdsSubscriber` — so we have on-project evidence that a nano-ros
DDS node interoperates with stock ROS 2. Contingent on having
CycloneDDS installed in the test environment; can gate on
`require_cyclonedds()` (similar to the existing
`require_rmw_zenoh()`).

### 71.10 — (Optional) Upstream `block_on` + non-blocking transport

71.1 and 71.2 are not nano-ros-specific — every dust-dds consumer
wanting a single-threaded / cooperative executor runs into the same
hardcoded `block_on` and the same 3-thread recv model. Open a PR
against `s2e-systems/dust-dds` once 71.1–71.4 are stable in the fork.
Keep `NrosPlatformRuntime<P>` / the arena hook local — those are
nano-ros-specific.

## Acceptance Criteria

- [ ] `packages/dds/nros-rmw-dds` ships `platform-posix`,
      `platform-zephyr`, `platform-nuttx`, `platform-threadx`,
      `platform-freertos`, and `platform-bare-metal` features,
      mirroring `nros-rmw-zenoh`'s matrix.
- [ ] `NrosPlatformRuntime<ConcretePlatform>` implements `DdsRuntime`
      by dispatching through `<P as PlatformX>::…`. No new platform
      primitives; no stand-alone runtime implementations.
- [ ] The dust-dds fork's UDP transport is non-blocking; `poll_recv()`
      is the only entry point into the network for nros consumers.
- [ ] `nros_node::Executor::spin_once()` drives DDS receive progress
      on every platform — no background threads owned by DDS on the
      nros side.
- [ ] Bare-metal QEMU DDS talker/listener passes E2E (see 71.7).
- [ ] Zephyr native_sim DDS talker/listener passes E2E (see 71.8).
- [ ] `nros-rmw-dds` + `nros-rmw-zenoh` coexist cleanly — a single
      node can instantiate either backend selected at compile time,
      and the examples/ tree has both a zenoh and a dds variant for
      every platform that supports both.
- [ ] `just ci` passes with all changes.

## Notes

- **CycloneDDS as a separate backend is out of scope.** Dust-dds is
  already wire-compatible with CycloneDDS (and Fast-DDS); adding a
  native CycloneDDS C backend would duplicate the RTPS implementation,
  bring heap-dependent POSIX-or-Zephyr-with-POSIX-shim back into the
  embedded build, and deliver zero interop benefit. See the dust-dds
  `interoperability_tests/cyclone_dds/` CI suite for the evidence.
- **Fork maintenance**: keep the delta against upstream dust-dds small
  and additive (new modules only). The two non-additive patches
  (71.1 `block_on` on `DdsRuntime`, 71.2 non-blocking transport) are
  worth upstreaming (71.10). `NrosPlatformRuntime<P>` lives in
  `nros-rmw-dds`, not the fork.
- **Memory budget**: provision 32–64 KiB heap for DDS on bare-metal.
  Cap `DataWriter` / `DataReader` history with `KEEP_LAST(depth=1)` to
  keep per-topic footprint bounded. Document per-deployment heap
  requirements in `book/src/user-guide/rmw-backends.md`.
- **Discovery on bare-metal**: SPDP uses UDP multicast on
  `239.255.0.1:7400..7500`. `nros_smoltcp::SmoltcpBridge` already
  supports multicast group join (`zpico-platform-shim` uses it for
  zenoh's own multicast scouting). SPDP announcement period defaults
  to 30 s; increase on highly constrained networks.
- **Cooperative executor latency**: all DDS progress happens inside
  `Executor::spin_once()`. Apps must call it at ≤100 ms intervals to
  keep reliability timers happy; for cleanly-terminating message
  handlers this is automatic, but long-running handlers should either
  yield periodically or run in a separate arena task.
- **Alloc-free claim**: nano-ros's "alloc-free on bare-metal" guarantee
  applies to `nros-rmw-zenoh` + `nros-rmw-xrce` only. `nros-rmw-dds`
  requires a `#[global_allocator]` on every platform it runs on. This
  is a deliberate split between backends, documented on each backend's
  landing page. 71.6 adds the opt-in allocator on supported board
  crates.
