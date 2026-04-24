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

- [ ] 71.1 — `DdsRuntime::block_on` back on the trait (fork patch)
- [ ] 71.2 — Non-blocking UDP transport — replace std-thread model
- [x] 71.3 — `NrosPlatformRuntime<P>` adapter (`nros-rmw-dds::runtime`)
- [ ] 71.4 — Arena entry for RTPS receive poll (`nros-rmw-dds::session`)
- [x] 71.5 — Feature-gated backend selection in `nros-rmw-dds`
- [ ] 71.6 — Board-crate `#[global_allocator]` support (off by default)
- [ ] 71.7 — Bare-metal QEMU DDS talker/listener example + nextest suite
- [ ] 71.8 — Zephyr DDS talker/listener example + nextest suite
- [ ] 71.9 — (Optional) CycloneDDS / Fast-DDS interop test in nros-tests
- [ ] 71.10 — (Optional) Upstream `block_on` + non-blocking transport to dust-dds

### 71.1 — `DdsRuntime::block_on` back on the trait

Sync `DomainParticipantFactory`, `DataWriter`, `DataReader`, etc. in the
fork currently import `dust_dds::std_runtime::executor::block_on`
directly. Reintroduce `fn block_on<T>(f: impl Future<Output = T>) -> T`
on the `DdsRuntime` trait (it was present in v0.14, removed in v0.15
upstream — we're keeping the fork closer to v0.14 on this point).

The default `StdRuntime` impl continues to use `thread::park` /
`unpark`; our new `NrosPlatformRuntime` spins `Executor::spin_once()`
until the future resolves (same shape as `nros_node::Promise::wait`).

**Files**:
- `packages/dds/dust-dds/dds/src/runtime.rs` — reintroduce trait method
- `packages/dds/dust-dds/dds/src/dds/**/*.rs` — replace direct
  `std_runtime::executor::block_on` calls with `R::block_on(...)`

### 71.2 — Non-blocking UDP transport

`dust_dds::rtps_udp_transport::RtpsUdpTransportParticipantFactory`
today spawns three `std::thread::spawn` recv loops per participant and
uses `block_on(sender.send())` inside each. Replace with:

1. Non-blocking sockets (`set_nonblocking(true)` on POSIX, `recv` with
   zero timeout on `PlatformUdp`).
2. A single `poll_recv()` function that tries each socket, returns
   `Option<(Locator, Bytes)>`, and is called from the spawner's task
   queue (nros platform) or from a dedicated recv task (std platform,
   unchanged).

On POSIX this means swapping the 3× thread model for 1× thread driving
`poll()` or `mio::Poll`. On nros platforms it means not spawning threads
at all — `recv` is polled cooperatively from `spin_once()` (71.4).

This is the one substantial delta against upstream dust-dds and is
worth contributing back (see 71.10).

**Files**:
- `packages/dds/dust-dds/dds/src/rtps_udp_transport/udp_transport.rs`
- `packages/dds/dust-dds/dds/src/rtps_udp_transport/mod.rs`

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

### 71.4 — Arena entry for RTPS receive poll

Mirrors the existing `zpico-platform-shim` dispatch hook: add an arena
entry to `nros_node::Executor` that, on each `spin_once()`, drains the
dust-dds runtime's task queue once and polls each participant's recv
sockets with a short timeout.

This is the key to unifying the model across platforms — dust-dds does
not own any threads on our side, and DDS progress is deterministic with
respect to the nros executor spin cadence.

**Files**:
- `packages/dds/nros-rmw-dds/src/session.rs` — `drive_io()` actually
  does something now; currently a no-op (`std_runtime` runs threads).
- `packages/core/nros-node/src/executor/` — possibly a new
  `ArenaEntryKind::DdsSession` (or reuse a generic poll hook — TBD at
  implementation time).

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
