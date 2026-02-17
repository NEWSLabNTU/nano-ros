# Phase 42 — Extensible RMW Layer

## Status: In Progress

## Background

nano-ros currently has two RMW backends — zenoh-pico (`nros-rmw-zenoh`) and
XRCE-DDS (`nros-rmw-xrce`) — each with its own duplicated node API:
`ShimNode`/`ShimExecutor` (zenoh) and `XrceNode`/`XrceExecutor` (XRCE). Both
implement the same patterns (typed pub/sub, services, actions) but with
backend-specific types, creating maintenance burden and blocking third-party
backends.

The `nros-rmw` crate already defines backend-agnostic traits (`Session`,
`Publisher`, `Subscriber`, `ServiceServerTrait`, `ServiceClientTrait`), but the
node-level API in `nros-node` doesn't use them generically — it hardcodes
backend types behind `#[cfg(feature = "rmw-zenoh")]` / `#[cfg(feature =
"rmw-xrce")]` feature gates.

This phase introduces:
1. A generic `EmbeddedNode<S>` / `EmbeddedExecutor<S>` that works with any
   `Session` implementation
2. A C function table adapter (`nros-rmw-cffi`) enabling backends written in C,
   C++, Zig, Ada, or any language with a C-compatible ABI
3. Deletion of the duplicated `ShimNode`/`XrceNode` APIs (no backward
   compatibility)

### Architecture

```
Third-party Rust backend:
  nros-rmw-foo (implements traits directly) --> nros-rmw traits

Third-party C backend:
  librmw_bar.a (C functions) --> nros-rmw-cffi (adapter) --> nros-rmw traits

Third-party C++/Zig/Ada backend:
  librmw_baz.a (C-compatible ABI) --> nros-rmw-cffi (adapter) --> nros-rmw traits
```

For Rust backends: zero-cost monomorphized dispatch, full compile-time type
safety. For C/other-language backends: function pointer indirection (~2-3 cycles
per call), `void*` type erasure at the boundary.

## Current Architecture

### Duplicated node APIs

| Type           | Zenoh (`shim.rs`)            | XRCE (`xrce.rs`)          |
|----------------|------------------------------|---------------------------|
| Executor       | `ShimExecutor`               | `XrceExecutor`            |
| Node           | `ShimNode`                   | `XrceNode`                |
| Publisher      | `ShimNodePublisher<M>`       | `XrceNodePublisher<M>`    |
| Subscription   | `ShimNodeSubscription<M, N>` | `XrceNodeSubscription<M>` |
| Service Server | `ShimNodeServiceServer`      | `XrceNodeServiceServer`   |
| Service Client | `ShimNodeServiceClient`      | `XrceNodeServiceClient`   |
| Action Server  | `ShimNodeActionServer`       | —                         |
| Action Client  | `ShimNodeActionClient`       | —                         |
| Error          | `ShimNodeError`              | `XrceNodeError`           |

Both implement the same patterns: typed handles wrapping raw backend handles,
CDR serialization/deserialization, buffer management, and action protocol state
machines. The zenoh backend additionally has action support that XRCE lacks.

### Missing `drive_io()` abstraction

The executor loop needs to poll the transport for I/O. Currently this is done
via backend-specific methods (`ShimSession::spin_once` / `XrceSession::spin_once`)
and a helper function `nros::internals::drive_session_io()` that dispatches via
feature gates. A generic executor needs this as a trait method.

## Phase 42.1 — Add `drive_io()` to Session Trait

Add a default `drive_io()` method to the `Session` trait so generic code can
drive transport I/O without knowing the backend type. Push-based backends use
the default no-op; pull-based backends (zenoh, XRCE) override it.

### Session trait addition

**File: `packages/core/nros-rmw/src/traits.rs`** (after `close()`)

```rust
fn drive_io(&mut self, timeout_ms: i32) -> Result<(), Self::Error> {
    let _ = timeout_ms;
    Ok(())
}
```

### Backend implementations

**File: `packages/zpico/nros-rmw-zenoh/src/shim.rs`** — add to
`impl Session for ShimSession`:

```rust
fn drive_io(&mut self, timeout_ms: i32) -> Result<(), Self::Error> {
    self.spin_once(timeout_ms as u32).map(|_| ())
}
```

**File: `packages/xrce/nros-rmw-xrce/src/lib.rs`** — add to
`impl Session for XrceSession`:

```rust
fn drive_io(&mut self, timeout_ms: i32) -> Result<(), Self::Error> {
    self.spin_once(timeout_ms);
    Ok(())
}
```

### Simplify `drive_session_io`

**File: `packages/core/nros/src/lib.rs`** — the existing
`internals::drive_session_io` helper can delegate to the trait method:

```rust
pub fn drive_session_io(session: &mut RmwSession, timeout_ms: i32) {
    use nros_rmw::Session;
    let _ = session.drive_io(timeout_ms);
}
```

### Tasks

- [ ] Add `drive_io()` default method to `Session` trait
- [ ] Implement `drive_io()` for `ShimSession` (zenoh)
- [ ] Implement `drive_io()` for `XrceSession` (XRCE)
- [ ] Update `internals::drive_session_io` to use trait method

## Phase 42.2 — Generic Embedded Node API

Create a backend-agnostic node and executor parameterized by `Session` trait,
replacing the duplicated `ShimNode`/`XrceNode` pattern. Full feature parity:
pub/sub, services, and actions.

**No backward compatibility** — `ShimNode`/`ShimExecutor` and
`XrceNode`/`XrceExecutor` (along with all their typed handle types) will be
deleted and replaced by the generic types directly.

### Error type

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EmbeddedNodeError {
    Transport(TransportError),
    NameTooLong,
    Serialization,
    BufferTooSmall,
    ActionCreationFailed,
    ServiceRequestFailed,
    ServiceReplyFailed,
}

impl From<TransportError> for EmbeddedNodeError { ... }
```

Uses `TransportError` (not generic over backend error) — both existing backends
map to it. Session trait calls use:

```rust
session.create_publisher(&topic, qos)
    .map_err(|_| EmbeddedNodeError::Transport(TransportError::PublisherCreationFailed))?;
```

### `EmbeddedExecutor<S>`

```rust
pub struct EmbeddedExecutor<S> { session: S }

impl<S: Session> EmbeddedExecutor<S> {
    pub fn from_session(session: S) -> Self;
    pub fn create_node(&mut self, name: &str) -> Result<EmbeddedNode<'_, S>, EmbeddedNodeError>;
    pub fn drive_io(&mut self, timeout_ms: i32) -> Result<(), EmbeddedNodeError>;
    pub fn session(&self) -> &S;
    pub fn session_mut(&mut self) -> &mut S;
}
```

### `EmbeddedNode<S>`

```rust
pub struct EmbeddedNode<'a, S: Session> {
    name: heapless::String<64>,
    session: &'a mut S,
    domain_id: u32,
}
```

Methods delegate to `session.create_*()` and wrap results in typed handles:

- `create_publisher<M>` / `create_publisher_with_qos<M>`
- `create_subscription<M>` / `create_subscription_sized<M, RX_BUF>` /
  `create_subscription_with_qos<M, RX_BUF>`
- `create_service<Svc>` / `create_service_sized<Svc, REQ, REPLY>`
- `create_client<Svc>` / `create_client_sized<Svc, REQ, REPLY>`
- `create_action_server<A>` /
  `create_action_server_sized<A, GOAL, RESULT, FEEDBACK, MAX_GOALS>`
- `create_action_client<A>` /
  `create_action_client_sized<A, GOAL, RESULT, FEEDBACK>`
- `session_mut()`, `name()`, `domain_id()`, `set_domain_id()`

### Typed handle types

Parameterized by the Session's associated handle types:

```rust
pub struct EmbeddedPublisher<M, P> {
    handle: P,
    _phantom: PhantomData<M>,
}

pub struct EmbeddedSubscription<M, Sub, const RX_BUF: usize> {
    handle: Sub,
    buffer: [u8; RX_BUF],
    _phantom: PhantomData<M>,
}

pub struct EmbeddedServiceServer<S, Srv, const REQ: usize, const REPLY: usize> {
    handle: Srv,
    req_buffer: [u8; REQ],
    reply_buffer: [u8; REPLY],
    _phantom: PhantomData<S>,
}

pub struct EmbeddedServiceClient<S, Cli, const REQ: usize, const REPLY: usize> {
    handle: Cli,
    req_buffer: [u8; REQ],
    reply_buffer: [u8; REPLY],
    _phantom: PhantomData<S>,
}
```

### Action types

Generic versions of `ShimNodeActionServer`/`ShimNodeActionClient` with full
protocol support (send_goal, cancel_goal, get_result services + feedback,
status topics):

```rust
pub struct EmbeddedActiveGoal<A: RosAction> {
    pub goal_id: [u8; 16],
    pub status: GoalStatus,
    pub goal: A::Goal,
}

pub struct EmbeddedCompletedGoal<A: RosAction> {
    pub goal_id: [u8; 16],
    pub status: GoalStatus,
    pub result: A::Result,
}

pub struct EmbeddedActionServer<
    A, Srv, Pub,
    const GOAL: usize, const RESULT: usize,
    const FEEDBACK: usize, const MAX_GOALS: usize,
> {
    send_goal_server: Srv,
    cancel_goal_server: Srv,
    get_result_server: Srv,
    feedback_publisher: Pub,
    _status_publisher: Pub,
    active_goals: heapless::Vec<EmbeddedActiveGoal<A>, MAX_GOALS>,
    completed_goals: heapless::Vec<EmbeddedCompletedGoal<A>, MAX_GOALS>,
    // buffers...
}

pub struct EmbeddedActionClient<
    A, Cli, Sub,
    const GOAL: usize, const RESULT: usize, const FEEDBACK: usize,
> {
    send_goal_client: Cli,
    cancel_goal_client: Cli,
    get_result_client: Cli,
    feedback_subscriber: Sub,
    // buffers...
}
```

Method impls use trait bounds: `P: Publisher`, `Sub: Subscriber`,
`Srv: ServiceServerTrait`, `Cli: ServiceClientTrait`. Backend errors are mapped
to `EmbeddedNodeError::Transport(TransportError::*)`.

### Module registration

**File: `packages/core/nros-node/src/lib.rs`** — add unconditionally (no
feature gate):

```rust
pub mod generic;

pub use generic::{
    EmbeddedExecutor, EmbeddedNode, EmbeddedNodeError,
    EmbeddedPublisher, EmbeddedSubscription,
    EmbeddedServiceServer, EmbeddedServiceClient,
    EmbeddedActionServer, EmbeddedActionClient,
    EmbeddedActiveGoal, EmbeddedCompletedGoal,
};
```

### Migration (no backward compatibility)

The old backend-specific node types are deleted entirely:

- **Deleted from `shim.rs`**: `ShimNode`, `ShimExecutor`,
  `ShimNodePublisher<M>`, `ShimNodeSubscription<M, N>`,
  `ShimNodeServiceServer`, `ShimNodeServiceClient`,
  `ShimNodeActionServer`, `ShimNodeActionClient`,
  `ShimActiveGoal`, `ShimCompletedGoal`, `ShimNodeError`
- **Deleted from `xrce.rs`**: `XrceNode`, `XrceExecutor`,
  `XrceNodePublisher<M>`, `XrceNodeSubscription<M>`,
  `XrceNodeServiceServer`, `XrceNodeServiceClient`, `XrceNodeError`
- All examples, board crates, and test code migrate to
  `EmbeddedExecutor<S>`, `EmbeddedNode<S>`, and the generic handle types

### Tasks

- [ ] Create `packages/core/nros-node/src/generic.rs` (~800 lines)
- [ ] Add `EmbeddedNodeError` type
- [ ] Add `EmbeddedExecutor<S>` with `from_session`, `create_node`, `drive_io`
- [ ] Add `EmbeddedNode<S>` with pub/sub/service/action creation methods
- [ ] Add typed handle types (`EmbeddedPublisher`, `EmbeddedSubscription`, etc.)
- [ ] Add action types (`EmbeddedActionServer`, `EmbeddedActionClient`, etc.)
- [ ] Add `pub mod generic` + re-exports to `nros-node/src/lib.rs`
- [ ] Delete `nros-node/src/shim.rs` and `nros-node/src/xrce.rs`
- [ ] Update all examples and board crates to use generic types

## Phase 42.3 — C Function Table (`nros-rmw-cffi`)

New crate providing a C-callable vtable interface so backends written in C,
C++, Zig, Ada, or any language with a C-compatible ABI can plug into nano-ros
without writing Rust code.

### Crate structure

```
packages/core/nros-rmw-cffi/
  Cargo.toml
  src/
    lib.rs          # Vtable struct, registration, CffiRmw factory
    session.rs      # CffiSession impl Session
    publisher.rs    # CffiPublisher impl Publisher
    subscriber.rs   # CffiSubscriber impl Subscriber
    service.rs      # CffiServiceServer, CffiServiceClient
    convert.rs      # QoS conversion, CStr helpers
  include/
    nros/
      rmw_vtable.h  # C header for backend implementors
```

### C header (`include/nros/rmw_vtable.h`)

```c
#ifndef NROS_RMW_VTABLE_H
#define NROS_RMW_VTABLE_H

#include <stdint.h>
#include <stddef.h>

typedef void* nros_rmw_handle_t;

typedef struct nros_rmw_cffi_qos_t {
    uint8_t reliability;  /* 0=BestEffort, 1=Reliable */
    uint8_t durability;   /* 0=Volatile, 1=TransientLocal */
    uint8_t history;      /* 0=KeepLast, 1=KeepAll */
    uint32_t depth;
} nros_rmw_cffi_qos_t;

typedef struct nros_rmw_vtable_t {
    /* Session lifecycle */
    nros_rmw_handle_t (*open)(const char *locator, uint8_t mode,
                              uint32_t domain_id, const char *node_name);
    int32_t (*close)(nros_rmw_handle_t session);
    int32_t (*drive_io)(nros_rmw_handle_t session, int32_t timeout_ms);

    /* Publisher */
    nros_rmw_handle_t (*create_publisher)(nros_rmw_handle_t session,
        const char *topic_name, const char *type_name, const char *type_hash,
        uint32_t domain_id, const nros_rmw_cffi_qos_t *qos);
    void (*destroy_publisher)(nros_rmw_handle_t publisher);
    int32_t (*publish_raw)(nros_rmw_handle_t publisher,
        const uint8_t *data, size_t len);

    /* Subscriber */
    nros_rmw_handle_t (*create_subscriber)(nros_rmw_handle_t session,
        const char *topic_name, const char *type_name, const char *type_hash,
        uint32_t domain_id, const nros_rmw_cffi_qos_t *qos);
    void (*destroy_subscriber)(nros_rmw_handle_t subscriber);
    int32_t (*try_recv_raw)(nros_rmw_handle_t subscriber,
        uint8_t *buf, size_t buf_len);
    int32_t (*has_data)(nros_rmw_handle_t subscriber);

    /* Service Server */
    nros_rmw_handle_t (*create_service_server)(nros_rmw_handle_t session,
        const char *service_name, const char *type_name, const char *type_hash,
        uint32_t domain_id);
    void (*destroy_service_server)(nros_rmw_handle_t server);
    int32_t (*try_recv_request)(nros_rmw_handle_t server,
        uint8_t *buf, size_t buf_len, int64_t *seq_out);
    int32_t (*has_request)(nros_rmw_handle_t server);
    int32_t (*send_reply)(nros_rmw_handle_t server,
        int64_t seq, const uint8_t *data, size_t len);

    /* Service Client */
    nros_rmw_handle_t (*create_service_client)(nros_rmw_handle_t session,
        const char *service_name, const char *type_name, const char *type_hash,
        uint32_t domain_id);
    void (*destroy_service_client)(nros_rmw_handle_t client);
    int32_t (*call_raw)(nros_rmw_handle_t client,
        const uint8_t *request, size_t req_len,
        uint8_t *reply_buf, size_t reply_buf_len);
} nros_rmw_vtable_t;

/* Register a custom RMW backend. Call before nano_ros_support_init(). */
int32_t nros_rmw_cffi_register(const nros_rmw_vtable_t *vtable);

#endif
```

### Return value conventions

| Function | Success | No data | Error |
|----------|---------|---------|-------|
| `open` | non-null handle | — | null |
| `close`, `drive_io`, `publish_raw`, `send_reply` | 0 | — | negative |
| `try_recv_raw` | positive (bytes received) | 0 | negative |
| `try_recv_request` | positive (bytes, seq_out written) | 0 | negative |
| `has_data`, `has_request` | 1 (yes) | 0 (no) | — |
| `call_raw` | positive (reply bytes) | — | negative |
| `destroy_*` | void (best-effort) | — | — |

### Rust adapter types

```rust
pub struct CffiSession { vtable: &'static NrosRmwVtable, handle: CffiHandle }
pub struct CffiPublisher { vtable: &'static NrosRmwVtable, handle: CffiHandle }
pub struct CffiSubscriber { vtable: &'static NrosRmwVtable, handle: CffiHandle }
pub struct CffiServiceServer { vtable: &'static NrosRmwVtable, handle: CffiHandle }
pub struct CffiServiceClient { vtable: &'static NrosRmwVtable, handle: CffiHandle }
```

Each implements the corresponding `nros-rmw` trait by calling through the
vtable. `Drop` impls call `destroy_*`.

### Registration

Uses `portable-atomic::AtomicPtr` static (`no_std` compatible):

```rust
static VTABLE: AtomicPtr<NrosRmwVtable> = AtomicPtr::new(core::ptr::null_mut());

#[unsafe(no_mangle)]
pub unsafe extern "C" fn nros_rmw_cffi_register(vtable: *const NrosRmwVtable) -> i32 {
    VTABLE.store(vtable as *mut _, Ordering::Release);
    0
}
```

### Factory

```rust
pub struct CffiRmw;

impl Rmw for CffiRmw {
    type Session = CffiSession;
    type Error = TransportError;

    fn open(config: &RmwConfig) -> Result<CffiSession, TransportError> { ... }
}
```

### Cargo.toml

```toml
[package]
name = "nros-rmw-cffi"
version.workspace = true
edition.workspace = true
license.workspace = true
description = "C function table adapter for nros RMW backends"

[lib]
name = "nros_rmw_cffi"

[dependencies]
nros-rmw = { workspace = true, default-features = false }
portable-atomic = { version = "1", default-features = false }

[features]
default = []
std = ["nros-rmw/std"]
alloc = ["nros-rmw/alloc"]
```

### Tasks

- [ ] Create `packages/core/nros-rmw-cffi/` crate structure
- [ ] Write C header (`include/nros/rmw_vtable.h`)
- [ ] Implement `CffiSession` (Session trait via vtable)
- [ ] Implement `CffiPublisher` (Publisher trait via vtable)
- [ ] Implement `CffiSubscriber` (Subscriber trait via vtable)
- [ ] Implement `CffiServiceServer` and `CffiServiceClient`
- [ ] Add vtable registration (`nros_rmw_cffi_register`)
- [ ] Add `CffiRmw` factory
- [ ] Add QoS conversion helpers

## Phase 42.4 — Feature Wiring

Wire the `rmw-cffi` feature through the crate stack with mutual exclusivity
against existing backends.

### Feature chain

**`packages/core/nros-node/Cargo.toml`:**

```toml
rmw-cffi = ["dep:nros-rmw-cffi"]

[dependencies]
nros-rmw-cffi = { path = "../nros-rmw-cffi", default-features = false, optional = true }
```

**`packages/core/nros/Cargo.toml`:**

```toml
rmw-cffi = ["dep:nros-rmw-cffi", "nros-node/rmw-cffi"]

[dependencies]
nros-rmw-cffi = { path = "../nros-rmw-cffi", default-features = false, optional = true }
```

**`packages/core/nros-c/Cargo.toml`:**

```toml
rmw-cffi = ["nros/rmw-cffi"]
```

### Mutual exclusivity

**File: `packages/core/nros/src/lib.rs`:**

```rust
#[cfg(all(feature = "rmw-cffi", feature = "rmw-zenoh"))]
compile_error!("`rmw-cffi` and `rmw-zenoh` are mutually exclusive.");
#[cfg(all(feature = "rmw-cffi", feature = "rmw-xrce"))]
compile_error!("`rmw-cffi` and `rmw-xrce` are mutually exclusive.");
```

### Type aliases

**File: `packages/core/nros/src/lib.rs`:**

```rust
#[cfg(feature = "rmw-cffi")]
pub type RmwSession = nros_rmw_cffi::CffiSession;
#[cfg(feature = "rmw-cffi")]
pub type RmwPublisher = nros_rmw_cffi::CffiPublisher;
#[cfg(feature = "rmw-cffi")]
pub type RmwSubscriber = nros_rmw_cffi::CffiSubscriber;
#[cfg(feature = "rmw-cffi")]
pub type RmwServiceServer = nros_rmw_cffi::CffiServiceServer;
#[cfg(feature = "rmw-cffi")]
pub type RmwServiceClient = nros_rmw_cffi::CffiServiceClient;
```

### `open_session` support

```rust
#[cfg(all(feature = "rmw-cffi", not(feature = "rmw-zenoh"), not(feature = "rmw-xrce")))]
{
    use nros_rmw::Rmw;
    let config = nros_rmw::RmwConfig {
        locator, mode, domain_id, node_name, namespace: "",
    };
    nros_rmw_cffi::CffiRmw::open(&config)
        .map_err(|_| nros_rmw::TransportError::ConnectionFailed)
}
```

### Workspace and CI

**Root `Cargo.toml`** — add workspace member:
`"packages/core/nros-rmw-cffi"`

**`justfile`** — add check recipe:
```
cargo clippy -p nros --features rmw-cffi --no-default-features -- -D warnings
```

### Tasks

- [ ] Add `rmw-cffi` feature + dep to `nros-node/Cargo.toml`
- [ ] Add `rmw-cffi` feature + dep to `nros/Cargo.toml`
- [ ] Add `rmw-cffi` feature to `nros-c/Cargo.toml`
- [ ] Add mutual exclusivity compile errors in `nros/src/lib.rs`
- [ ] Add `RmwSession`/`RmwPublisher`/etc. type aliases for cffi
- [ ] Add `open_session` cffi branch
- [ ] Add workspace member to root `Cargo.toml`
- [ ] Add clippy check recipe to `justfile`

## Implementation Order

1. Add `drive_io()` to Session trait + backend impls (42.1)
2. Create `generic.rs` with all types (42.2)
3. Add `pub mod generic` to `nros-node/src/lib.rs` (42.2)
4. Delete `shim.rs` and `xrce.rs`, migrate all usage to generic types (42.2)
5. Create `nros-rmw-cffi` crate (42.3)
6. Wire `rmw-cffi` feature (42.4)
7. Run `just quality`

## Verification

1. `just quality` — format + clippy + test (validates all existing backends
   still work)
2. `cargo clippy -p nros --features rmw-cffi --no-default-features -- -D warnings`
   — validates cffi compiles
3. `cargo clippy -p nros-node --no-default-features -- -D warnings` — validates
   generic module compiles without any backend
4. `gcc -fsyntax-only -Wall packages/core/nros-rmw-cffi/include/nros/rmw_vtable.h`
   — C header valid

## Key Files

| File                                                    | Role                                            |
|---------------------------------------------------------|-------------------------------------------------|
| `packages/core/nros-rmw/src/traits.rs`                  | Session trait: `drive_io()` addition            |
| `packages/zpico/nros-rmw-zenoh/src/shim.rs`             | Zenoh `drive_io()` impl                         |
| `packages/xrce/nros-rmw-xrce/src/lib.rs`                | XRCE `drive_io()` impl                          |
| `packages/core/nros-node/src/generic.rs`                | **New**: Generic embedded node API (~800 lines) |
| `packages/core/nros-node/src/shim.rs`                   | **Delete**: Zenoh-specific node types           |
| `packages/core/nros-node/src/xrce.rs`                   | **Delete**: XRCE-specific node types            |
| `packages/core/nros-node/src/lib.rs`                    | Module registration + re-exports                |
| `packages/core/nros-rmw-cffi/`                          | **New crate**: C vtable adapter (~600 lines)    |
| `packages/core/nros-rmw-cffi/include/nros/rmw_vtable.h` | C header for backend implementors               |
| `packages/core/nros/src/lib.rs`                         | Feature gates, type aliases, `open_session`     |
| `packages/core/nros/Cargo.toml`                         | `rmw-cffi` feature + dep                        |
| `packages/core/nros-node/Cargo.toml`                    | `rmw-cffi` feature + dep                        |
| `packages/core/nros-c/Cargo.toml`                       | `rmw-cffi` feature                              |
| Root `Cargo.toml`                                       | Workspace member addition                       |
