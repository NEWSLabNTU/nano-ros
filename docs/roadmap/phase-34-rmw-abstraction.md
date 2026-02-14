# Phase 34: RMW Abstraction & XRCE-DDS Integration

**Status: Not Started**

**Prerequisites:** Phase 33 (crate rename) must be complete — this phase uses `nros-*` / `zpico-*` names.

**Design docs:**
- `docs/design/rmw-layer-design.md` — Overall architecture and RMW layer design
- `docs/reference/rmw-h-analysis.md` — ROS 2 rmw.h analysis (limitations, what to adopt)
- `docs/reference/xrce-dds-analysis.md` — XRCE-DDS feasibility analysis

## Goal

1. Formalize the `nros-rmw` trait interface so board crates (`nros-qemu`, etc.) are truly middleware-agnostic
2. Refactor board crates to use `nros-rmw` traits instead of calling `zenoh_shim_*` FFI directly
3. Implement XRCE-DDS as the second RMW backend, proving the abstraction works

## Current State

Board crates call zenoh-pico FFI directly in two ways:

1. **`zenoh_shim_*` FFI calls** (~10 functions in `node.rs`): `zenoh_shim_open_session`, `zenoh_shim_declare_publisher`, `zenoh_shim_put`, etc. — bypasses any trait abstraction
2. **Keyexpr formatting** (~20 lines in `node.rs`): formats zenoh-specific key expressions like `<domain>/<topic>/<type>/TypeHashNotSupported`

Until these are replaced with trait-based calls through `nros-rmw`, no alternative middleware can work.

## Steps

### 34.1: Formalize `nros-rmw` trait interface

Add factory traits to `nros-rmw/src/traits.rs` (design in `rmw-layer-design.md` "RMW Trait Changes"):

```rust
/// Factory for creating middleware sessions
pub trait Rmw {
    type Session: Session;
    fn open(config: &RmwConfig) -> Result<Self::Session, RmwError>;
}

/// Middleware session
pub trait Session {
    type Publisher: Publisher;
    type Subscriber: Subscriber;
    type ServiceServer: ServiceServer;
    type ServiceClient: ServiceClient;

    fn create_publisher(&mut self, topic: &TopicInfo, qos: &QosSettings)
        -> Result<Self::Publisher, RmwError>;
    fn create_subscriber(&mut self, topic: &TopicInfo, qos: &QosSettings)
        -> Result<Self::Subscriber, RmwError>;
    fn create_service_server(&mut self, service: &ServiceInfo, qos: &QosSettings)
        -> Result<Self::ServiceServer, RmwError>;
    fn create_service_client(&mut self, service: &ServiceInfo, qos: &QosSettings)
        -> Result<Self::ServiceClient, RmwError>;
    fn spin_once(&self, timeout_ms: u32) -> Result<(), RmwError>;
    fn is_open(&self) -> bool;
}
```

Key design decisions from `rmw-h-analysis.md`:
- **No heap allocation** in trait methods (unlike rmw.h's `rmw_create_*` returning heap pointers)
- **Non-blocking `spin_once`** instead of rmw.h's blocking `rmw_wait`
- **Raw bytes interface** (`publish_raw(&[u8])`, `try_recv_raw(&mut [u8])`) — serialization is above the RMW layer
- **Minimal QoS** (reliability, durability, history depth) — not rmw.h's full DDS QoS model
- **No graph introspection** in core traits — requires `alloc`, optional for embedded

Also define:
- `RmwConfig` — generic middleware configuration (must accommodate both zenoh locator strings and XRCE-DDS agent IP+port)
- `RmwError` — error enum covering both backends
- `TopicInfo`, `ServiceInfo` — middleware-agnostic, no `to_key()` methods (keyexpr formatting stays in `nros-rmw-zenoh`)

### 34.2: Implement `nros-rmw-zenoh` traits

Make `nros-rmw-zenoh` implement the formalized `Rmw` and `Session` traits from 34.1:

```rust
// nros-rmw-zenoh/src/lib.rs
pub struct ZenohRmw;
pub struct ZenohSession { /* wraps zpico-sys session handle */ }
pub struct ZenohPublisher { /* wraps zpico-sys publisher handle */ }
// ...

impl Rmw for ZenohRmw {
    type Session = ZenohSession;
    fn open(config: &RmwConfig) -> Result<ZenohSession, RmwError> {
        // Parse zenoh locator from config
        // Call zenoh_shim_open_session
    }
}

impl Session for ZenohSession {
    type Publisher = ZenohPublisher;
    // ...
    fn create_publisher(&mut self, topic: &TopicInfo, qos: &QosSettings)
        -> Result<ZenohPublisher, RmwError> {
        // Format zenoh keyexpr from TopicInfo
        // Call zenoh_shim_declare_publisher
    }
}
```

This absorbs the `zenoh_shim_*` calls that are currently scattered in board crate `node.rs` files.

### 34.3: Refactor board crates to use `nros-rmw` traits

Replace direct `zenoh_shim_*` FFI in each board crate with trait-based calls:

**Before (current):**
```rust
// nros-qemu/src/node.rs
unsafe { zenoh_shim_open_session(locator.as_ptr(), mode) };
let key = format_keyexpr(domain, topic, type_name);  // zenoh-specific
unsafe { zenoh_shim_declare_publisher(key.as_ptr()) };
unsafe { zenoh_shim_put(publisher_id, data.as_ptr(), data.len()) };
```

**After:**
```rust
// nros-qemu/src/node.rs
let session = ZenohRmw::open(&config)?;
let publisher = session.create_publisher(&topic_info, &qos)?;
publisher.publish_raw(data)?;
```

Board crates become generic over `R: Rmw`:
```rust
pub struct Node<R: Rmw> {
    session: R::Session,
    // ...
}
```

Or, for embedded (no dynamic dispatch), use a concrete type alias:
```rust
#[cfg(feature = "zenoh")]
type DefaultRmw = nros_rmw_zenoh::ZenohRmw;
#[cfg(feature = "xrce")]
type DefaultRmw = nros_rmw_xrce::XrceRmw;
```

Each board crate:
- Remove all `zenoh_shim_*` FFI calls from `node.rs`
- Remove keyexpr formatting code
- Use `nros-rmw` trait methods exclusively
- Select middleware backend via Cargo feature flag

### 34.4: Create `xrce-sys` (FFI bindings)

Create FFI bindings to the [Micro-XRCE-DDS-Client](https://github.com/eProsima/Micro-XRCE-DDS-Client) C library.

```
packages/xrce/xrce-sys/
  Cargo.toml
  build.rs           # Compile Micro-XRCE-DDS-Client from submodule
  src/lib.rs          # Raw FFI bindings (#[repr(C)] types, extern "C" functions)
  Micro-XRCE-DDS-Client/  # Git submodule
```

Key API to bind:
- Session: `uxr_init_session`, `uxr_create_session`, `uxr_delete_session`, `uxr_run_session_time`
- Streams: `uxr_create_output_reliable_stream`, `uxr_create_input_reliable_stream`
- Entities: `uxr_buffer_create_participant_bin`, `_topic_bin`, `_publisher_bin`, `_datawriter_bin`, `_subscriber_bin`, `_datareader_bin`
- Data: `uxr_prepare_output_stream`, `uxr_buffer_request_data`, `uxr_set_topic_callback`
- Services: `uxr_buffer_create_requester_bin`, `uxr_buffer_create_replier_bin`, `uxr_buffer_request`, `uxr_buffer_reply`

### 34.5: Create `xrce-smoltcp` (UDP transport)

Implement XRCE-DDS custom transport callbacks using smoltcp UDP sockets.

```
packages/xrce/xrce-smoltcp/
  Cargo.toml
  src/lib.rs          # 4 callbacks: open, close, read, write
```

XRCE-DDS custom transport interface (4 callbacks):
```c
open_func  → open UDP socket via smoltcp
close_func → close UDP socket
write_func → send UDP datagram
read_func  → receive UDP datagram with timeout
```

This is much simpler than `zpico-smoltcp` (8+ TCP socket management functions) because XRCE-DDS uses UDP (connectionless) rather than TCP.

### 34.6: Create `nros-rmw-xrce` (RMW trait implementation)

Implement `nros-rmw` traits for XRCE-DDS.

```
packages/xrce/nros-rmw-xrce/
  Cargo.toml
  src/
    lib.rs            # XrceRmw, XrceSession
    session.rs        # Session management + stream creation
    publisher.rs      # DDS entity hierarchy (participant + topic + publisher + datawriter)
    subscriber.rs     # Datareader + request_data + callback buffer
    service.rs        # Requester/replier patterns
```

Key implementation challenges:
1. **Multi-step entity creation**: `create_publisher` must orchestrate 4 XRCE calls internally (participant, topic, publisher, datawriter)
2. **Callback-based subscription**: XRCE delivers data via `uxr_set_topic_callback`. Need to buffer received data for `try_recv_raw`
3. **Explicit data request**: Must call `uxr_buffer_request_data` after creating datareader, and re-request after processing
4. **Session event loop**: `spin_once` maps to `uxr_run_session_time(timeout)` which processes both pub and sub

### 34.7: Create `xrce-platform-qemu` (platform support)

Minimal platform crate for XRCE-DDS on QEMU. Much smaller than `zpico-platform-qemu` because XRCE-DDS needs only ~6 platform symbols vs zenoh-pico's 55.

```
packages/xrce/xrce-platform-qemu/
  Cargo.toml
  src/lib.rs          # clock_gettime (for session sync)
```

### 34.8: Integration testing

Test XRCE-DDS backend with the [Micro-XRCE-DDS Agent](https://github.com/eProsima/Micro-XRCE-DDS-Agent):

1. **QEMU + Agent**: Run XRCE agent on host, QEMU board crate connects via UDP
2. **Cross-backend**: nano-ros (zenoh) ↔ nano-ros (XRCE-DDS) via DDS bridge
3. **ROS 2 interop**: micro-ROS Agent bridges to ROS 2 DDS network

Test infrastructure additions:
- `XrceAgent` fixture in `nano-ros-tests/src/fixtures/` — manages agent process lifecycle
- New test suite: `nano-ros-tests/tests/xrce.rs`

## Platform Requirements Comparison

| Requirement | zenoh-pico (current) | XRCE-DDS |
|---|---|---|
| Client heap | Required (~16KB+) | **None** (fully static) |
| Client RAM | ~16KB+ | ~3KB |
| Client Flash | ~100KB+ | ~75KB |
| Platform symbols | ~55 FFI exports | ~6 callbacks |
| Transport impl complexity | 8+ TCP functions | 4 UDP callbacks |
| Bridge process | zenohd (optional for peer mode) | Agent (**mandatory**) |

## `RmwConfig` Design

Must accommodate both backends:

```rust
pub struct RmwConfig<'a> {
    /// Middleware-specific connection string
    /// zenoh: "tcp/192.168.1.1:7447"
    /// XRCE-DDS: "udp/192.168.1.1:2019" (agent address)
    pub locator: &'a str,

    /// Session mode (middleware-interpreted)
    /// zenoh: "client" or "peer"
    /// XRCE-DDS: ignored (always client)
    pub mode: &'a str,

    /// ROS 2 domain ID
    pub domain_id: u32,

    /// Node name
    pub node_name: &'a str,

    /// Node namespace
    pub namespace: &'a str,
}
```

## Estimated Effort

| Step | Description | Effort |
|------|-------------|--------|
| 34.1 | Formalize nros-rmw traits | 3 days |
| 34.2 | Implement nros-rmw-zenoh traits | 1 week |
| 34.3 | Refactor board crates | 1 week |
| 34.4 | xrce-sys FFI bindings | 1 week |
| 34.5 | xrce-smoltcp UDP transport | 3 days |
| 34.6 | nros-rmw-xrce implementation | 2 weeks |
| 34.7 | xrce-platform-qemu | 2 days |
| 34.8 | Integration testing | 1 week |
| **Total** | | **~7-8 weeks** |

## Ordering Notes

- **34.1-34.3 first**: Formalize traits and refactor board crates before starting XRCE-DDS. This validates the abstraction with the existing zenoh backend.
- **34.4-34.7 independent**: XRCE-DDS crates can be developed in parallel once traits are stable.
- **34.3 is the hardest step**: Board crates have deep zenoh assumptions. Must handle both `zenoh_shim_*` FFI calls and keyexpr formatting.
- **34.8 requires external infrastructure**: Micro-XRCE-DDS Agent must be built and available in CI.
