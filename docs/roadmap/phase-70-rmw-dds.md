# Phase 70 — DDS RMW Backend (dust-dds)

**Goal**: Add a DDS/RTPS-based RMW backend (`nros-rmw-dds`) using the pure-Rust
`dust-dds` library, giving nano-ros brokerless peer-to-peer communication that
interoperates with all major DDS implementations (Cyclone DDS, Fast DDS,
Connext, OpenDDS).

**Status**: Not Started

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

[dust-dds](https://github.com/s2e-systems/dust-dds) (v0.15.0) is a pure-Rust
DDS implementation. Key properties:

- **`no_std + alloc`** — feature-gated; core DDS logic runs without `std`
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

### Comparison with Zenoh

| Aspect           | rmw-zenoh               | rmw-dds                                     |
|------------------|-------------------------|---------------------------------------------|
| Discovery        | Router or peer          | Peer-to-peer multicast (brokerless)         |
| Transport        | TCP, UDP, serial        | UDP multicast + unicast                     |
| Implementation   | zenoh-pico (C FFI)      | dust-dds (pure Rust)                        |
| `no_std`         | Yes (via zpico-sys)     | Yes (`no_std + alloc`)                      |
| Wire interop     | rmw_zenoh_cpp           | rmw_cyclonedds, rmw_fastrtps, any DDS       |
| Binary size      | ~60 KB (zenoh-pico)     | TBD (estimate ~100-200 KB)                  |
| Heap usage       | ~16 KB                  | Moderate (history buffers, discovery state) |
| Serial transport | Built-in (COBS framing) | Not built-in (could implement custom)       |

## Architecture

### Crate Layout

```
packages/dds/
├── nros-rmw-dds/
│   ├── Cargo.toml
│   └── src/
│       ├── lib.rs          # Re-exports, feature gates
│       ├── session.rs      # DdsSession: impl Session trait
│       ├── publisher.rs    # DdsPublisher: impl Publisher trait
│       ├── subscriber.rs   # DdsSubscriber: impl Subscriber trait
│       ├── service.rs      # DdsServiceServer, DdsServiceClient
│       ├── transport.rs    # DdsRmw: impl Rmw trait (factory)
│       ├── runtime.rs      # NrosRuntime: impl DdsRuntime for nano-ros
│       ├── config.rs       # DDS domain config, QoS mapping
│       └── keyexpr.rs      # ROS 2 topic name ↔ DDS topic mapping
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
       └─ DomainParticipantFactoryAsync::create_participant()
            ├─ SPDP discovery starts (multicast)
            └─ Returns DdsSession (owns participant + runtime)

DdsSession::create_publisher(topic)
  └─ participant.create_publisher().create_datawriter()
       ├─ SEDP announces writer
       └─ Returns DdsPublisher (wraps DataWriter)

DdsSession::create_subscriber(topic)
  └─ participant.create_subscriber().create_datareader()
       ├─ SEDP announces reader
       └─ Returns DdsSubscriber (wraps DataReader)

DdsSession::drive_io(timeout_ms)
  └─ Poll dust-dds async runtime (process incoming RTPS messages,
     run discovery, dispatch to reader buffers)
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

## Work Items

- [ ] 70.1 — Create `nros-rmw-dds` crate skeleton
- [ ] 70.2 — Implement `DdsRuntime` for std targets
- [ ] 70.3 — Implement `Session` trait (`DdsSession`)
- [ ] 70.4 — Implement `Publisher` / `Subscriber` traits
- [ ] 70.5 — Implement service request/reply
- [ ] 70.6 — Wire into `nros-node` feature flags
- [ ] 70.7 — Wire into `nros-c` and `nros-cpp-ffi`
- [ ] 70.8 — Create native POSIX examples (talker/listener)
- [ ] 70.9 — Create Zephyr example (talker/listener)
- [ ] 70.10 — Integration tests (Rust ↔ Rust, C ↔ Rust)
- [ ] 70.11 — ROS 2 interop test (nano-ros DDS ↔ rmw_cyclonedds)

### 70.1 — Create `nros-rmw-dds` crate skeleton

Set up the crate with feature-gated dust-dds dependency. The crate follows
the same feature pattern as `nros-rmw-zenoh`:

```toml
[features]
default = []
std = ["dust_dds/std"]
platform-posix = ["std"]
platform-zephyr = []
ros-humble = []
ros-iron = []
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

Implement the `dust_dds::runtime::DdsRuntime` trait for POSIX/std
environments. dust-dds ships `StdRuntime` which we can use directly for
initial bringup, then replace with a custom implementation if needed.

For `no_std` targets, implement `NrosRuntime` backed by platform-specific
clock/timer/spawner primitives. This is deferred until Zephyr integration
(70.9).

**Files**:
- `packages/dds/nros-rmw-dds/src/runtime.rs`

### 70.3 — Implement `Session` trait (`DdsSession`)

Map `nros-rmw::Session` to dust-dds `DomainParticipantAsync`:

- `create_publisher()` → `participant.create_publisher().create_datawriter()`
- `create_subscriber()` → `participant.create_subscriber().create_datareader()`
- `create_service_server()` → pair of reader (request) + writer (reply)
- `close()` → `participant.delete_contained_entities()` + drop
- `drive_io()` → poll the dust-dds async runtime

Key design decision: dust-dds is async-first but nano-ros executors are
synchronous (polling). `drive_io()` must run the dust-dds event loop for
`timeout_ms` and return. Use a block-on executor or poll-once approach.

**Files**:
- `packages/dds/nros-rmw-dds/src/session.rs`
- `packages/dds/nros-rmw-dds/src/config.rs`

### 70.4 — Implement `Publisher` / `Subscriber` traits

`DdsPublisher` wraps a dust-dds `DataWriterAsync`. Serialization uses
nano-ros's existing CDR encoder (`nros-serdes`) — publish raw CDR bytes
via `write()`.

`DdsSubscriber` wraps a `DataReaderAsync`. `has_data()` checks the reader
status; `try_recv_raw()` calls `take()` to consume one sample.

Topic naming: apply `rt/` prefix and DDS type name mangling
(`pkg::msg::dds_::Type_`).

**Files**:
- `packages/dds/nros-rmw-dds/src/publisher.rs`
- `packages/dds/nros-rmw-dds/src/subscriber.rs`
- `packages/dds/nros-rmw-dds/src/keyexpr.rs`

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

**Files**:
- `packages/core/nros-node/Cargo.toml` — add `rmw-dds` feature + dependency
- `packages/core/nros-node/src/session.rs` — add `ConcreteSession` alias
- `packages/core/nros/Cargo.toml` — forward `rmw-dds`
- `Cargo.toml` (workspace) — add `nros-rmw-dds` member

### 70.7 — Wire into `nros-c` and `nros-cpp-ffi`

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
- `packages/core/nros-cpp-ffi/Cargo.toml`
- `packages/core/nros-cpp-ffi/src/lib.rs` — add `rmw-dds` to cfg gate
- `CMakeLists.txt` — add `dds` option
- `packages/core/nros-c/CMakeLists.txt` — map `dds` to feature

### 70.8 — Create native POSIX examples (talker/listener)

Create Rust talker and listener examples using `rmw-dds` + `platform-posix`.
These use UDP multicast for discovery — no router or agent needed.

```toml
# examples/native/rust/dds/talker/Cargo.toml
[dependencies]
nros = { path = "...", features = ["std", "rmw-dds", "platform-posix"] }
```

Also create C talker/listener examples using CMake + `NANO_ROS_RMW=dds`.

**Files**:
- `examples/native/rust/dds/talker/`
- `examples/native/rust/dds/listener/`
- `examples/native/c/dds/talker/`
- `examples/native/c/dds/listener/`

### 70.9 — Create Zephyr example (talker/listener)

Create a Zephyr DDS example targeting a board with Ethernet (e.g.,
`fvp_baser_aemv8r` or `native_sim`). Requires implementing `NrosRuntime`
for Zephyr (clock from `k_uptime_get()`, timer from `k_timer`, spawner
from `k_thread_create`).

This validates `no_std + alloc` DDS operation on an RTOS.

**Files**:
- `examples/zephyr/rust/dds/talker/`
- `examples/zephyr/rust/dds/listener/`

### 70.10 — Integration tests (Rust ↔ Rust, C ↔ Rust)

Add tests to `nros-tests` exercising the DDS backend:

- Rust talker → Rust listener (same process, two executors)
- C talker → C listener (two processes)
- Cross-language: Rust talker → C listener

These use loopback UDP (127.0.0.1 multicast) — no external processes needed.

**Files**:
- `packages/testing/nros-tests/tests/dds_api.rs`
- `tests/dds-talker-listener.sh`

### 70.11 — ROS 2 interop test (nano-ros DDS ↔ rmw_cyclonedds)

Verify that a nano-ros DDS node can communicate with a standard ROS 2 node
using `rmw_cyclonedds_cpp` or `rmw_fastrtps_cpp`. This requires:

- Matching DDS domain ID
- Correct `rt/` topic prefix and type name mangling
- CDR-compatible serialization
- SPDP/SEDP discovery on the same multicast group

Similar to the existing `just test-ros2` recipe but using `rmw-dds` instead
of `rmw-zenoh`.

**Files**:
- `tests/dds-ros2-interop.sh`
- `justfile` — add `test-dds-ros2` recipe

## Acceptance Criteria

- [ ] `nros-rmw-dds` crate compiles with `no_std + alloc` (no `std` feature)
- [ ] Native POSIX talker/listener exchange messages over DDS/UDP
- [ ] C API talker/listener work with `NANO_ROS_RMW=dds`
- [ ] Two nano-ros DDS nodes discover each other without a router
- [ ] nano-ros DDS node communicates with a ROS 2 `rmw_cyclonedds` node
- [ ] `just quality` passes with `rmw-dds` feature enabled
- [ ] Zephyr example compiles and runs on a supported board

## Notes

- **Async bridging**: dust-dds is async-first. The `drive_io()` method must
  bridge async/sync. For `std`, a minimal block-on executor can poll futures.
  For `no_std`, the platform's event loop drives the DDS runtime.

- **Memory budget**: dust-dds uses `alloc` extensively (Vec, Arc, Box).
  History buffer sizes should be capped via QoS `KEEP_LAST` with small depth
  to bound memory usage on embedded targets. Document heap requirements.

- **Discovery overhead**: SPDP/SEDP multicast uses ~1-2 KB per announcement
  at configurable intervals (default 5s). On constrained networks, increase
  the interval or use static peer configuration.

- **No serial transport**: Unlike zenoh-pico, DDS/RTPS is designed for UDP.
  Serial transport would require encapsulating RTPS frames over a serial link
  with custom framing — possible but non-standard.

- **dust-dds `todo!()` stubs**: ~30 async API methods are unimplemented in
  dust-dds v0.15. Verify that the methods needed by nano-ros (create
  participant, create writer/reader, write, take) are all functional before
  starting integration.

- **CDR compatibility**: nano-ros already serializes messages to CDR via
  `nros-serdes`. dust-dds has its own CDR codec via `DdsType` derive. For
  raw pub/sub (`publish_raw` / `try_recv_raw`), bypass dust-dds serialization
  and pass pre-encoded CDR bytes directly.
