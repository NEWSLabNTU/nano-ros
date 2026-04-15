# Phase 71 — Refactor dust-dds to Platform-Agnostic Architecture

**Goal**: Refactor the forked dust-dds library so that all platform services
(clock, sockets, threading, RNG) are provided through pluggable interfaces,
enabling bare-metal and RTOS support without modifying core DDS logic.

**Status**: Not Started

**Priority**: Medium

**Depends on**: Phase 70 (DDS RMW backend — POSIX working)

## Overview

dust-dds upstream is async-native at the DCPS/core level — all DDS logic
(discovery, reliability, message dispatch) is generic over the `DdsRuntime`
trait. However, two layers break platform independence:

1. **Transport** (`rtps_udp_transport/`) — spawns 3 OS threads that call
   `socket.recv()` blocking and push data via `std_runtime::executor::block_on()`.
   Hardcoded to `socket2` + `std::thread`.

2. **Sync API** (`dds/`) — imports `std_runtime::executor::block_on` directly.
   Every sync method wraps an async call with OS-level blocking.

The goal is to make dust-dds follow the same pattern as zenoh-pico and
Micro-XRCE-DDS: the library defines **system service interfaces**, and the
platform fills them in.

### Current Architecture (std-only)

```
┌─────────────────────────────────────────┐
│ dust-dds core (async, runtime-agnostic) │
│   dcps/ dds_async/ rtps/ xtypes/        │
│   Generic over R: DdsRuntime            │
├─────────────────────────────────────────┤
│ StdRuntime                              │  ← hardcoded in sync API + transport
│   Executor thread (std::thread)         │
│   Timer thread (std::thread)            │
│   StdClock (SystemTime)                 │
├─────────────────────────────────────────┤
│ RtpsUdpTransportParticipantFactory      │  ← hardcoded to socket2 + 3 OS threads
│   3x recv threads (blocking)            │
│   block_on(sender.send()) in threads    │
└─────────────────────────────────────────┘
```

### Target Architecture (platform-agnostic)

```
┌─────────────────────────────────────────┐
│ dust-dds core (async, runtime-agnostic) │
│   dcps/ dds_async/ rtps/ xtypes/        │
│   Generic over R: DdsRuntime            │
│   Generic over T: TransportFactory      │
├───────────────┬─────────────────────────┤
│ std platform  │ bare-metal platform     │
│  StdRuntime   │  NrosRuntime            │
│  StdClock     │  HardwareClock (FFI)    │
│  OS threads   │  Cooperative executor   │
│  socket2 UDP  │  smoltcp UDP            │
│  SystemTime   │  Cycle counter          │
└───────────────┴─────────────────────────┘
```

### Design Principles

Following the zenoh-pico / XRCE-DDS pattern:

1. **Library defines interfaces, platform implements them.** dust-dds core
   never calls `std::thread::spawn`, `socket2::Socket::recv`, or
   `SystemTime::now` directly.

2. **Non-blocking I/O model.** The transport provides `recv()` as
   non-blocking (returns `Option`). The caller drives I/O by calling a poll
   function, same as zenoh-pico's `zpico_spin_once()` or nros's `drive_io()`.

3. **Cooperative execution.** On bare-metal, all DDS work (discovery,
   reliability, message dispatch) happens when the application calls
   `drive_io()`. No background threads. On std, a full-threaded runtime
   can still be used for maximum throughput.

4. **Static or heap allocation.** Core logic uses `alloc` (Vec, Arc).
   The platform provides `#[global_allocator]` on bare-metal. nros core
   crates remain heap-free.

## Platform Service Interfaces

### Clock

```rust
pub trait Clock: Clone + Send + Sync + 'static {
    fn now(&self) -> Time;
}
```

| Platform | Implementation |
|----------|---------------|
| POSIX | `SystemTime::now()` |
| Bare-metal | Hardware timer / cycle counter via FFI |
| Zephyr | `k_uptime_get()` |
| FreeRTOS | `xTaskGetTickCount()` |

### Timer / Delay

```rust
pub trait Timer: Clone + Send + Sync + 'static {
    fn delay(&mut self, duration: Duration) -> impl Future<Output = ()> + Send;
}
```

| Platform | Implementation |
|----------|---------------|
| POSIX | `StdTimer` (dedicated timer thread + binary heap) |
| Bare-metal | Deadline-based future, checked in cooperative poll loop |
| Zephyr | `k_timer` based |

### Task Spawner

```rust
pub trait Spawner: Clone + Send + Sync + 'static {
    fn spawn(&self, f: impl Future<Output = ()> + Send + 'static);
}
```

| Platform | Implementation |
|----------|---------------|
| POSIX | `StdExecutor` (dedicated thread + task queue) |
| Bare-metal | Static task pool, polled from `drive_io()` |

### Block-on (sync bridge)

Currently hardcoded in the sync API. Must become platform-provided:

```rust
pub trait BlockOn {
    fn block_on<T>(f: impl Future<Output = T>) -> T;
}
```

| Platform | Implementation |
|----------|---------------|
| POSIX | `thread::park()` / `unpark()` based |
| Bare-metal | Busy-poll loop with `drive_io()` integration |

### Transport (network I/O)

The `TransportParticipantFactory` trait already exists and is pluggable.
The issue is that the built-in UDP implementation hardcodes OS threads
and `block_on`. The interface itself is correct:

```rust
pub trait TransportParticipantFactory: Send + 'static {
    fn create_participant(
        &self,
        domain_id: i32,
        data_channel_sender: MpscSender<Arc<[u8]>>,
    ) -> impl Future<Output = RtpsTransportParticipant> + Send;
}

pub trait WriteMessage: Send + Sync {
    fn write_message(&self, buf: &[u8], locators: &[Locator])
        -> Pin<Box<dyn Future<Output = ()> + Send>>;
}
```

New transports to implement:
- **Non-blocking POSIX UDP** — uses `mio` or `poll()` instead of blocking threads
- **smoltcp UDP** — for bare-metal, reuses nano-ros's smoltcp infrastructure

## Work Items

- [ ] 71.1 — Make sync API `block_on` pluggable
- [ ] 71.2 — Refactor UDP transport to non-blocking
- [ ] 71.3 — Implement `NrosRuntime` (cooperative executor for bare-metal)
- [ ] 71.4 — Implement smoltcp UDP transport (`dds-smoltcp` crate)
- [ ] 71.5 — Wire `NrosRuntime` into `nros-rmw-dds` for `no_std` targets
- [ ] 71.6 — Board crate `#[global_allocator]` support for DDS
- [ ] 71.7 — Bare-metal QEMU examples + integration tests
- [ ] 71.8 — Zephyr DDS examples + integration tests

### 71.1 — Make sync API `block_on` pluggable

The sync `DomainParticipantFactory` and all sync entity types (`DataWriter`,
`DataReader`, etc.) import `std_runtime::executor::block_on` directly.

Patch: either add `block_on` to the `DdsRuntime` trait, or make the sync
API generic over a `BlockOn` trait. The simplest approach is adding
`fn block_on<T>(f: impl Future<Output = T>) -> T` back to `DdsRuntime`
(it was present in v0.14, removed in v0.15).

**Files**:
- `packages/dds/dust-dds/dds/src/runtime.rs` — add `block_on` to trait
- `packages/dds/dust-dds/dds/src/dds/` — replace `std_runtime::executor::block_on`
  with `R::block_on()` in all sync wrappers

### 71.2 — Refactor UDP transport to non-blocking

Replace the 3 blocking OS threads in `RtpsUdpTransportParticipantFactory`
with non-blocking socket reads. The receive path becomes:

```
Old: 3x std::thread::spawn → socket.recv() [blocking] → block_on(send)
New: transport.poll_recv() → socket.recv() [non-blocking] → send to channel
```

The factory's `create_participant()` returns an `RtpsTransportParticipant`
with a `poll_recv` method (or integrates with the spawner for async recv).

On POSIX, use non-blocking sockets with `poll()` or `mio`. The existing
`StdRuntime` executor can still drive them, or nros's `drive_io()` can
poll directly.

**Files**:
- `packages/dds/dust-dds/dds/src/rtps_udp_transport/udp_transport.rs` —
  refactor to non-blocking
- Remove `use std_runtime::executor::block_on` from transport

### 71.3 — Implement `NrosRuntime` (cooperative executor for bare-metal)

A single-threaded cooperative runtime for `no_std + alloc`:

- **Clock**: extern FFI function provided by board crate
- **Timer**: Deadline-based futures checked in poll loop
- **Spawner**: Heap-allocated task list (`Vec<Pin<Box<dyn Future>>>`)
  polled round-robin from `drive_io()`
- **Channels**: Already `no_std` compatible (use `critical_section`)

The executor is polled from `DdsSession::drive_io()`, which nano-ros calls
from `Executor::spin_once()`.

**Files**:
- `packages/dds/dust-dds/dds/src/nros_runtime/` — new module
  - `mod.rs` — `NrosRuntime` struct
  - `executor.rs` — cooperative task executor
  - `timer.rs` — deadline-based timer
  - `clock.rs` — FFI clock bridge

### 71.4 — Implement smoltcp UDP transport (`dds-smoltcp` crate)

Implement `TransportParticipantFactory` using smoltcp UDP sockets. Reuses
the nano-ros smoltcp infrastructure (LAN9118 driver, static socket storage).

Requirements:
- UDP unicast send/recv (data traffic)
- UDP multicast join + recv (SPDP discovery on `239.255.0.1`)
- Non-blocking recv (polled from `drive_io()`)
- Configurable fragment size (default 1344 bytes)

**Files**:
- `packages/dds/dds-smoltcp/Cargo.toml`
- `packages/dds/dds-smoltcp/src/lib.rs`

### 71.5 — Wire `NrosRuntime` into `nros-rmw-dds` for `no_std` targets

Update `nros-rmw-dds` to use:
- `StdRuntime` + `RtpsUdpTransportParticipantFactory` when `std` is enabled
- `NrosRuntime` + `SmoltcpTransportParticipantFactory` when `no_std`

The `DdsSession::drive_io()` becomes a real poll loop on `no_std` (currently
a no-op because `StdRuntime` uses background threads).

**Files**:
- `packages/dds/nros-rmw-dds/src/transport.rs` — cfg-gated runtime selection
- `packages/dds/nros-rmw-dds/src/session.rs` — `drive_io()` polls executor
- `packages/dds/nros-rmw-dds/Cargo.toml` — optional dep on `dds-smoltcp`

### 71.6 — Board crate `#[global_allocator]` support for DDS

Add a Rust `#[global_allocator]` backed by a static heap to board crates
that support DDS. This provides the heap that dust-dds's `alloc` types use.

Uses `embedded-alloc` crate (maintained by rust-embedded team). Configured
with 32-64 KB heap depending on target RAM.

**Files**:
- `packages/boards/nros-mps2-an385/src/alloc.rs` — allocator setup
- `packages/boards/nros-mps2-an385/Cargo.toml` — optional `embedded-alloc` dep

### 71.7 — Bare-metal QEMU examples + integration tests

Create ARM bare-metal DDS examples using QEMU MPS2-AN385 with LAN9118
Ethernet. Same TAP bridge pattern as zenoh QEMU tests.

**Examples**:
- `examples/qemu-arm-baremetal/rust/dds/talker/`
- `examples/qemu-arm-baremetal/rust/dds/listener/`

**Tests**:
- Bare-metal DDS talker → listener over TAP bridge
- Bare-metal DDS ↔ native POSIX DDS (cross-platform interop)

**Files**:
- `tests/dds-qemu-talker-listener.sh`
- `packages/testing/nros-tests/tests/dds_qemu.rs`

### 71.8 — Zephyr DDS examples + integration tests

Create Zephyr DDS examples targeting a board with Ethernet. The Zephyr
platform provides clock (`k_uptime_get`), timer (`k_timer`), and threading
(`k_thread_create`) — these map to either `StdRuntime` (via Zephyr's POSIX
shim) or a Zephyr-specific `DdsRuntime`.

**Examples**:
- `examples/zephyr/rust/dds/talker/`
- `examples/zephyr/rust/dds/listener/`

## Acceptance Criteria

- [ ] dust-dds core compiles with `no_std + alloc` (no `std` imports in dcps/dds_async/rtps)
- [ ] Sync API uses pluggable `block_on` (not hardcoded to `std_runtime`)
- [ ] UDP transport uses non-blocking I/O (no OS threads for recv)
- [ ] `NrosRuntime` cooperative executor works on bare-metal
- [ ] smoltcp UDP transport sends/receives RTPS messages
- [ ] Bare-metal QEMU example exchanges messages over DDS
- [ ] Bare-metal DDS ↔ POSIX DDS cross-platform communication works
- [ ] `just ci` passes with all changes

## Notes

- **Fork maintenance**: All patches should be structured as clean commits
  on the fork branch. Track upstream dust-dds releases and rebase when
  beneficial. Prefer additive changes (new modules) over modifications to
  existing code to minimize merge conflicts.

- **Upstream contribution**: The platform-agnostic transport refactoring
  (71.2) could be contributed upstream as it benefits all dust-dds users.
  The `NrosRuntime` (71.3) is nano-ros-specific and stays in the fork.

- **Memory budget**: Bare-metal deployments should provision 32-64 KB heap
  for DDS. Cap history buffer sizes via QoS `KEEP_LAST` depth=1. Document
  per-deployment heap requirements.

- **Discovery on bare-metal**: SPDP uses UDP multicast (`239.255.0.1`).
  smoltcp supports multicast group joins. Discovery announcements happen
  every 30s by default — increase on constrained networks.

- **Cooperative executor latency**: All DDS work happens in `drive_io()`.
  If the application doesn't call it frequently enough, discovery and
  reliability timers will lag. Document minimum poll frequency
  (recommendation: ≤100ms between `drive_io()` calls).
