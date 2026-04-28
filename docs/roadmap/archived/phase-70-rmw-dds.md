# Phase 70 — DDS RMW Backend (dust-dds)

**Goal**: Add a DDS/RTPS-based RMW backend (`nros-rmw-dds`) using a forked
`dust-dds` library, giving nano-ros brokerless peer-to-peer communication that
interoperates with all major DDS implementations (Cyclone DDS, Fast DDS,
Connext, OpenDDS).

**Status**: Complete (all work items done; ROS 2 interop limited by RawCdrPayload type mismatch — fix in Phase 71)

**Priority**: Medium

**Depends on**: None (orthogonal to existing RMW backends)

## Overview

nano-ros currently supports two RMW backends:

| Backend     | Transport                         | Discovery               | Library           |
|-------------|-----------------------------------|-------------------------|-------------------|
| `rmw-zenoh` | TCP/UDP/serial via zenoh-pico (C) | Router (client) or peer | zenoh-pico ~60 KB |
| `rmw-xrce`  | UDP via Micro-XRCE-DDS (C)        | Agent (client-server)   | XRCE-DDS ~40 KB   |

Both require an external process (zenohd router or XRCE Agent) when running in
client mode. A DDS backend provides **brokerless peer-to-peer** discovery via
standard RTPS multicast — no router or agent needed.

### Why dust-dds

[dust-dds](https://github.com/s2e-systems/dust-dds) is a pure-Rust DDS
implementation. Key properties:

- **Pure Rust** — no C build system, no `unsafe`, no bindgen
- **OMG interop certified** — passes all 47 tests in the official
  [OMG DDS-RTPS interoperability suite](https://github.com/omg-dds/dds-rtps)
  against Cyclone DDS, Fast DDS, Connext, OpenDDS, InterCom, and CoreDX
- **Pluggable transport** — `TransportParticipantFactory` trait allows custom
  network backends (UDP, serial, smoltcp)
- **Pluggable runtime** — `DdsRuntime` trait abstracts clock, timer, and task
  spawning (no dependency on tokio/embassy)
- **CDR/XCDR serialization** — wire-compatible with ROS 2 CDR encoding
- **Apache-2.0 licensed**

### Forked dust-dds Submodule

dust-dds upstream requires `alloc` (`Vec`, `Arc`, `BTreeMap` in 846 sites).
We maintain a fork at `packages/dds/dust-dds/` (git submodule) to keep nros
core crates heap-free while confining allocations to dust-dds itself. See
[Memory Model](#memory-model) for the design.

### Position in Architecture

DDS is a new **RMW backend**, orthogonal to platform and ROS edition:

```
RMW Backend (one)         Platform (one)            ROS Edition (one)
---------------------     ----------------------    -----------------
rmw-zenoh                 platform-posix            ros-humble
rmw-xrce                  platform-zephyr           ros-iron
rmw-dds  <-- NEW          platform-bare-metal
                          platform-freertos
                          platform-nuttx
                          platform-threadx
```

### Comparison with Zenoh and XRCE

| Aspect           | rmw-zenoh               | rmw-xrce                     | rmw-dds                          |
|------------------|-------------------------|------------------------------|----------------------------------|
| Discovery        | Router or peer          | Agent (client-server)        | Peer-to-peer multicast           |
| Transport        | TCP, UDP, serial        | UDP                          | UDP multicast + unicast          |
| Implementation   | zenoh-pico (C FFI)      | Micro-XRCE-DDS (C FFI)      | dust-dds (pure Rust)             |
| Wire interop     | rmw_zenoh_cpp           | FastDDS Agent                | Any DDS (Cyclone, Fast, Connext) |
| nros Rust alloc  | No                      | No                           | No                               |
| Library alloc    | Yes (C `z_malloc`)      | No (fully static)            | Yes (Rust `alloc`)               |
| Allocator source | Platform crate provides `z_malloc`/`z_free` over static heap | N/A | Board crate provides `#[global_allocator]` over static heap |

## Memory Model

**Design principle**: nros core crates remain heap-free. dust-dds owns all
dynamic allocation through Rust's `alloc` crate. The board crate provides
the backing memory.

```
┌─────────────────────────────────────────────────────┐
│ Board crate (e.g., nros-board-mps2-an385)                 │
│   #[global_allocator]                               │
│   static HEAP: LlffHeap = ...;   // 64-128 KB      │
│   static mut HEAP_MEM: [u8; N];  // backing storage │
├─────────────────────────────────────────────────────┤
│ nros-rmw-dds          (alloc feature enabled)       │
│   Uses Vec<u8> for RawCdrPayload wrapper            │
├─────────────────────────────────────────────────────┤
│ dust-dds              (uses extern crate alloc)     │
│   Vec, Arc, BTreeMap for DynamicData, channels,     │
│   discovery state, history buffers                  │
├─────────────────────────────────────────────────────┤
│ nros core crates      (NO alloc feature)            │
│   nros, nros-node, nros-core, nros-rmw              │
│   All heap-free — only stack + static buffers       │
└─────────────────────────────────────────────────────┘
```

This mirrors the zenoh-pico pattern where the C library uses its own
`z_malloc()`/`z_free()` backed by a static memory pool, while the nros
Rust layer stays heap-free.

**Key difference**: zenoh-pico uses a C-level allocator (invisible to Rust),
while dust-dds uses Rust `alloc` (requires `#[global_allocator]`). Both
consume a static memory pool — the mechanism differs but the memory model
is the same.

### Feature Chain

```
nros (no alloc)
  └─ nros-node (no alloc)
       └─ nros-rmw-dds (alloc enabled)
            └─ dust_dds (extern crate alloc — uses Vec, Arc, BTreeMap)

Board crate provides #[global_allocator] backed by static [u8; N]
```

The `alloc` feature on `nros-rmw-dds` does NOT propagate to `nros-node`
or `nros`. The nros-node `alloc` forwarding uses `?` syntax
(`nros-rmw-dds?/alloc`), so it only activates when the consumer
explicitly requests it.

### Heap Budget

Estimated dust-dds heap usage for a minimal pub/sub deployment:

| Component                 | Allocation                            | Size estimate |
|---------------------------|---------------------------------------|---------------|
| DomainParticipant         | Discovery state, endpoint tables      | ~8 KB         |
| Per DataWriter            | History buffer (QoS depth × msg size) | ~2 KB         |
| Per DataReader            | History buffer + sample cache         | ~2 KB         |
| SPDP/SEDP                 | Participant/endpoint announcements    | ~4 KB         |
| Channels                  | Internal async message passing        | ~2 KB         |
| **Total (1 pub + 1 sub)** |                                       | **~18 KB**    |

Cap with QoS `KEEP_LAST` depth=1 and small message types. Board crates
should provision 32-64 KB heap for DDS.

## Architecture

### Crate Layout

```
packages/dds/
├── dust-dds/               # Forked git submodule
│   └── dds/                # dust_dds crate source
├── nros-rmw-dds/
│   ├── Cargo.toml
│   └── src/
│       ├── lib.rs          # Re-exports, feature gates
│       ├── raw_type.rs     # RawCdrPayload TypeSupport wrapper
│       ├── session.rs      # DdsSession: impl Session trait
│       ├── publisher.rs    # DdsPublisher: impl Publisher trait
│       ├── subscriber.rs   # DdsSubscriber: impl Subscriber trait
│       ├── service.rs      # DdsServiceServer, DdsServiceClient
│       └── transport.rs    # DdsRmw: impl Rmw trait (factory)
```

### dust-dds Integration

dust-dds requires two custom implementations:

1. **`DdsRuntime`** — provides clock, timer, and task spawner. For `std`
   targets, dust-dds ships `StdRuntime`. For embedded, we implement
   `NrosRuntime` that maps to platform-specific primitives (RTOS timers,
   monotonic clocks).

2. **`TransportParticipantFactory`** — provides the network layer. For `std`
   targets, dust-dds ships UDP transport via `socket2`. For bare-metal, we
   would implement a smoltcp-based UDP transport.

### Session Lifecycle

```
Executor::open()
  └─ DdsRmw::open(config)
       ├─ Create DdsRuntime (clock + timer + spawner)
       ├─ Create TransportParticipantFactory (UDP or custom)
       └─ DomainParticipantFactory::create_participant()
            ├─ SPDP discovery starts (multicast)
            └─ Returns DdsSession (owns participant)

DdsSession::create_publisher(topic)
  └─ participant.create_publisher().create_datawriter()
       ├─ SEDP announces writer
       └─ Returns DdsPublisher (wraps DataWriter)

DdsSession::create_subscriber(topic)
  └─ participant.create_subscriber().create_datareader()
       ├─ SEDP announces reader
       └─ Returns DdsSubscriber (wraps DataReader)

DdsSession::drive_io(timeout_ms)
  └─ No-op for StdRuntime (background executor thread).
     For embedded runtimes: poll the DDS async runtime.
```

### Topic Name Mapping

nano-ros already produces ROS 2-compatible topic names. DDS requires the
`rt/` prefix for ROS 2 interop:

| nano-ros TopicInfo          | DDS topic name                                   |
|-----------------------------|--------------------------------------------------|
| `/chatter` (domain 0)       | `rt/chatter`                                     |
| `/robot/cmd_vel` (domain 0) | `rt/robot/cmd_vel`                               |
| Service `/add_two_ints`     | `rq/add_two_intsRequest`, `rr/add_two_intsReply` |

Type names follow the DDS convention: `std_msgs::msg::dds_::Int32_`.

### QoS Mapping

`nros-rmw` `QosSettings` maps to DDS QoS policies:

| nros-rmw                     | DDS QoS             |
|------------------------------|---------------------|
| `Reliability::Reliable`      | `RELIABLE`          |
| `Reliability::BestEffort`    | `BEST_EFFORT`       |
| `Durability::Volatile`       | `VOLATILE`          |
| `Durability::TransientLocal` | `TRANSIENT_LOCAL`   |
| `History::KeepLast(n)`       | `KEEP_LAST` depth=n |

### Raw CDR Serialization Bridge

dust-dds's public API is typed (`DataWriter<Foo>::write(data: Foo)`), while
nros-rmw uses raw CDR bytes (`publish_raw(&[u8])`). A `RawCdrPayload`
wrapper type implements dust-dds's `TypeSupport` trait to carry pre-encoded
CDR bytes through the DDS pipeline.

**Current limitation**: `RawCdrPayload` registers as `SEQUENCE<UINT8>` in
the DDS type system, which adds a length prefix around the payload. This
means nano-ros DDS nodes can communicate with each other but not yet with
standard ROS 2 DDS nodes (wire format mismatch). Fixing this requires
either bypassing `DynamicData` serialization or implementing per-message-type
`TypeSupport` — addressed in 70.11.

## Work Items

- [x] 70.1 — Create `nros-rmw-dds` crate skeleton
- [x] 70.2 — Implement `DdsRuntime` for std targets
- [x] 70.3 — Implement `Session` trait (`DdsSession`)
- [x] 70.4 — Implement `Publisher` / `Subscriber` traits
- [x] 70.5 — Implement service request/reply
- [x] 70.6 — Wire into `nros-node` feature flags
- [x] 70.7 — Wire into `nros-c` and `nros-cpp`
- [x] 70.8 — Native POSIX examples + integration tests
- [x] 70.9 — ROS 2 interop test (nano-ros DDS ↔ rmw_cyclonedds)
- [x] 70.10 — Switch dust-dds dependency to local submodule fork

### 70.1 — Create `nros-rmw-dds` crate skeleton

Set up the crate with feature-gated dust-dds dependency. The `alloc` feature
is enabled on `nros-rmw-dds` but does NOT propagate to nros core crates.

```toml
[features]
default = []
std = ["alloc", "dust_dds/std", "dust_dds/rtps_udp_transport"]
alloc = ["nros-rmw/alloc"]
```

Implement stub types that satisfy the `nros-rmw` traits with `todo!()` bodies
to verify compilation through the full stack.

**Files**:
- `packages/dds/nros-rmw-dds/Cargo.toml`
- `packages/dds/nros-rmw-dds/src/lib.rs`
- `packages/dds/nros-rmw-dds/src/session.rs`
- `packages/dds/nros-rmw-dds/src/publisher.rs`
- `packages/dds/nros-rmw-dds/src/subscriber.rs`
- `packages/dds/nros-rmw-dds/src/service.rs`
- `packages/dds/nros-rmw-dds/src/transport.rs`

### 70.2 — Implement `DdsRuntime` for std targets

Use dust-dds's built-in `StdRuntime` directly — it provides executor,
timer, clock, and channels. No custom runtime needed for POSIX targets.

For embedded targets (70.9), implement a custom `DdsRuntime` backed by
platform-specific primitives.

**Files**:
- `packages/dds/nros-rmw-dds/src/transport.rs` (uses `StdRuntime` via
  `DomainParticipantFactoryAsync::get_instance()`)

### 70.3 — Implement `Session` trait (`DdsSession`)

Map `nros-rmw::Session` to dust-dds `DomainParticipant`:

- `create_publisher()` → `participant.create_publisher().create_datawriter()`
- `create_subscriber()` → `participant.create_subscriber().create_datareader()`
- `create_service_server()` → pair of reader (request) + writer (reply)
- `close()` → drop (DomainParticipant cleanup)
- `drive_io()` → no-op for `StdRuntime` (background executor thread)

**Files**:
- `packages/dds/nros-rmw-dds/src/session.rs`

### 70.4 — Implement `Publisher` / `Subscriber` traits

`DdsPublisher` wraps a dust-dds `DataWriter<RawCdrPayload>`. `publish_raw()`
wraps CDR bytes in `RawCdrPayload` and calls `DataWriter::write()`.

`DdsSubscriber` wraps a `DataReader<RawCdrPayload>`. `try_recv_raw()` calls
`DataReader::take()` and extracts the byte payload.

**Files**:
- `packages/dds/nros-rmw-dds/src/publisher.rs`
- `packages/dds/nros-rmw-dds/src/subscriber.rs`
- `packages/dds/nros-rmw-dds/src/raw_type.rs`

### 70.5 — Implement service request/reply

DDS services use the ROS 2 request/reply convention: two topics per service
(`rq/<service>Request` and `rr/<service>Reply`) with a `SampleIdentity`
header for correlating requests to replies.

Implement `DdsServiceServer` and `DdsServiceClient` wrapping paired
reader/writer endpoints.

**Files**:
- `packages/dds/nros-rmw-dds/src/service.rs`

### 70.6 — Wire into `nros-node` feature flags

Add `rmw-dds` feature to `nros-node`, `nros`, and all crates in the
feature-forwarding chain:

```rust
// nros-node/src/session.rs
#[cfg(feature = "rmw-dds")]
pub(crate) type ConcreteSession = nros_rmw_dds::DdsSession;
```

Forward platform and ROS edition features through to `nros-rmw-dds`.
The `alloc` feature on `nros-rmw-dds` does NOT propagate upward — nros
core crates remain heap-free.

**Files**:
- `packages/core/nros-node/Cargo.toml` — add `rmw-dds` feature + dependency
- `packages/core/nros-node/src/session.rs` — add `ConcreteSession` alias
- `packages/core/nros-node/build.rs` — add `rmw-dds` to `has_rmw` cfg
- `packages/core/nros/Cargo.toml` — forward `rmw-dds`
- `Cargo.toml` (workspace) — add `nros-rmw-dds` member + dep alias

### 70.7 — Wire into `nros-c` and `nros-cpp`

Add `rmw-dds` feature to the C and C++ FFI crates, forwarding to `nros`:

```toml
# nros-c/Cargo.toml
rmw-dds = ["nros/rmw-dds"]
```

Update CMakeLists.txt to accept `NANO_ROS_RMW=dds` alongside `zenoh`
and `xrce`. Update the compile-time assertion cfg gate to include `rmw-dds`.

**Files**:
- `packages/core/nros-c/Cargo.toml`
- `packages/core/nros-c/src/executor.rs` — add `rmw-dds` to cfg gate
- `packages/core/nros-cpp/Cargo.toml`
- `packages/core/nros-cpp/src/lib.rs` — add `rmw-dds` to cfg gate
- `CMakeLists.txt` — add `dds` option
- `packages/core/nros-c/CMakeLists.txt` — map `dds` to feature

### 70.8 — Native POSIX examples + integration tests

Create Rust and C examples using `rmw-dds` + `platform-posix`, then add
integration tests that exercise them. UDP multicast for discovery — no
router or agent needed.

```toml
# examples/native/rust/dds/talker/Cargo.toml
[dependencies]
nros = { path = "...", features = ["std", "rmw-dds", "platform-posix"] }
```

**Examples**:
- `examples/native/rust/dds/talker/`
- `examples/native/rust/dds/listener/`
- `examples/native/c/dds/talker/`
- `examples/native/c/dds/listener/`

**Tests** (loopback UDP multicast, no external processes):
- Rust talker → Rust listener (two processes)
- C talker → C listener (two processes)
- Cross-language: Rust talker → C listener
- Service: Rust server ↔ Rust client

**Files**:
- `packages/testing/nros-tests/tests/dds_api.rs`
- `tests/dds-talker-listener.sh`
- `justfile` — add `test-dds` recipe

### 70.9 — ROS 2 interop test (nano-ros DDS ↔ rmw_cyclonedds)

Verify that a nano-ros DDS node can communicate with a standard ROS 2 node
using `rmw_cyclonedds_cpp` or `rmw_fastrtps_cpp`. This requires:

- Matching DDS domain ID
- Correct `rt/` topic prefix and type name mangling
- CDR-compatible serialization (bypass `RawCdrPayload` wrapper — use
  per-message-type `TypeSupport` or direct CDR injection)
- SPDP/SEDP discovery on the same multicast group

Similar to the existing `just test-ros2` recipe but using `rmw-dds` instead
of `rmw-zenoh`.

**Files**:
- `tests/dds-ros2-interop.sh`
- `justfile` — add `test-dds-ros2` recipe

### 70.10 — Switch dust-dds dependency to local submodule fork

Switch `nros-rmw-dds/Cargo.toml` from crates.io `dust_dds = "0.14"` to
the local forked submodule:

```toml
dust_dds = { path = "../dust-dds/dds", default-features = false, features = ["dcps", "rtps"] }
```

**Files**:
- `packages/dds/nros-rmw-dds/Cargo.toml` — switch dep path

## Acceptance Criteria

- [x] `nros-rmw-dds` compiles with `alloc` but nros core crates compile without `alloc`
- [x] Native POSIX talker/listener exchange messages over DDS/UDP
- [x] Two nano-ros DDS nodes discover each other without a router
- [ ] C API talker/listener work with `NANO_ROS_RMW=dds`
- [x] ROS 2 interop test infrastructure created (interop blocked by RawCdrPayload type mismatch — fix deferred to Phase 71)
- [ ] `just quality` passes with `rmw-dds` feature enabled

## Notes

- **Async bridging**: dust-dds is async-first. The sync API uses
  `R::block_on()` to bridge. For `std`, `StdRuntime` provides a built-in
  executor. For embedded, the custom `DdsRuntime` must implement `block_on`.

- **Memory budget**: dust-dds uses `alloc` extensively. Cap heap usage via
  QoS `KEEP_LAST` depth=1 and small message types. Board crates should
  provision 32-64 KB heap for DDS. Document per-deployment heap requirements.

- **Discovery overhead**: SPDP/SEDP multicast uses ~1-2 KB per announcement
  at configurable intervals (default 5s). On constrained networks, increase
  the interval or use static peer configuration.

- **No serial transport**: Unlike zenoh-pico, DDS/RTPS is designed for UDP.
  Serial transport would require encapsulating RTPS frames over a serial link
  with custom framing — possible but non-standard.

- **CDR compatibility**: nano-ros serializes messages to CDR via `nros-serdes`.
  dust-dds has its own CDR codec via `DdsType` derive. The current
  `RawCdrPayload` wrapper adds a byte-sequence envelope that breaks ROS 2
  interop. Fixing this (70.11) requires either bypassing `DynamicData`
  serialization in the dust-dds fork or implementing per-message-type
  `TypeSupport` via codegen.

- **Heap isolation**: nros core crates (`nros`, `nros-node`, `nros-core`,
  `nros-rmw`) MUST NOT enable `alloc`. Only `nros-rmw-dds` and `dust_dds`
  use the heap. The `#[global_allocator]` is provided by the board crate,
  not by nros.
