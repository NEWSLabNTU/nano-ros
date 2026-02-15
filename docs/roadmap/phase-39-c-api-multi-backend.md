# Phase 39: C API Multi-Backend Support

**Status: Not Started**

**Prerequisites:** Phase 34 (RMW abstraction — complete), Phase 38 (example cleanup — complete)

## Goal

Make `nros-c` support multiple RMW backends (zenoh-pico, XRCE-DDS) using the same `nros-rmw` trait abstraction that the Rust API already uses. Today `nros-c` is hardcoded to zenoh-pico — every `_init` function imports `nros_rmw_zenoh::Shim*` types directly and casts them through `*mut c_void` pointers. This phase replaces those concrete types with trait-object dispatch so the C library works with any backend selected at compile time.

## Current State

### Coupling points

`nros-c` imports 5 concrete zenoh types across 5 source files:

| File              | Zenoh type                                              | Usage                                                               |
|-------------------|---------------------------------------------------------|---------------------------------------------------------------------|
| `support.rs`      | `ShimSession`                                           | Created in `nano_ros_support_init`, stored as `*mut c_void`         |
| `publisher.rs`    | `ShimSession`, `ShimPublisher`                          | Session for `create_publisher`, publisher stored as `*mut c_void`   |
| `subscription.rs` | `ShimSession`, `ShimSubscriber`                         | Session for `create_subscriber`, subscriber stored as `*mut c_void` |
| `service.rs`      | `ShimSession`, `ShimServiceServer`, `ShimServiceClient` | Session for create, handles stored as `*mut c_void`                 |
| `executor.rs`     | `ShimSubscriber`, `ShimServiceServer`                   | Cast from `*mut c_void` for `recv_raw()` and `check_ready()`        |

All imports are `use nros_rmw_zenoh::*` — `nros-c` never uses the `nros_rmw` traits for creation, only for method calls after casting.

### What already works

- The `nros-rmw` crate defines backend-agnostic traits: `Session`, `Publisher`, `Subscriber`, `ServiceServerTrait`, `ServiceClientTrait`
- `nros-rmw-zenoh` implements all traits for `ShimSession`, `ShimPublisher`, `ShimSubscriber`, `ShimServiceServer`, `ShimServiceClient`
- `nros-rmw-xrce` implements `Session` for `XrceSession` and pub/sub/service traits
- The C API feature flags (`rmw-zenoh`, `platform-*`, `ros-*`) are already orthogonal

### The problem

The `Session` trait uses associated types (`type PublisherHandle`, `type SubscriberHandle`, etc.), making it non-object-safe. You can't write `Box<dyn Session>` because the compiler doesn't know the concrete handle types. Each `_init` function must know the exact session type to call `create_publisher()` and get back a handle it can store.

## Design

### Approach: Backend type aliases with `cfg` dispatch

Use conditional type aliases (not trait objects) to select the concrete backend at compile time. This is the same pattern the Rust `nros` crate uses for `shim_aliases`.

```rust
// nros-c/src/backend.rs

#[cfg(feature = "rmw-zenoh")]
mod inner {
    pub type Session = nros_rmw_zenoh::ShimSession;
    pub type Publisher = nros_rmw_zenoh::ShimPublisher;
    pub type Subscriber = nros_rmw_zenoh::ShimSubscriber;
    pub type ServiceServer = nros_rmw_zenoh::ShimServiceServer;
    pub type ServiceClient = nros_rmw_zenoh::ShimServiceClient;
}

#[cfg(feature = "rmw-xrce")]
mod inner {
    pub type Session = nros_rmw_xrce::XrceSession;
    pub type Publisher = nros_rmw_xrce::XrcePublisher;
    pub type Subscriber = nros_rmw_xrce::XrceSubscriber;
    pub type ServiceServer = nros_rmw_xrce::XrceServiceServer;
    pub type ServiceClient = nros_rmw_xrce::XrceServiceClient;
}

pub use inner::*;
```

All `_init` and `_fini` functions replace `nros_rmw_zenoh::ShimSession` with `backend::Session`, `nros_rmw_zenoh::ShimPublisher` with `backend::Publisher`, etc.

**Why not trait objects?** Trait objects (`Box<dyn Session>`) are not possible because `Session` has associated types and is not object-safe. Trait objects also add vtable overhead inappropriate for embedded targets. Compile-time monomorphization via `cfg` is zero-cost and matches how the Rust API works.

### Session creation abstraction

The `nano_ros_support_init` function currently calls `ShimSession::new(&config)`. Each backend has a different constructor:

- zenoh: `ShimSession::new(&TransportConfig)`
- XRCE: `XrceSession::new(transport, agent_addr, ...)`

Introduce a thin `backend::open_session()` function in `backend.rs` that wraps the backend-specific constructor behind a common signature:

```rust
// nros-c/src/backend.rs

pub fn open_session(locator: &str, mode: SessionMode) -> Result<Session, TransportError> {
    #[cfg(feature = "rmw-zenoh")]
    {
        let config = TransportConfig {
            locator: Some(locator),
            mode,
            properties: &[],
        };
        Session::new(&config).map_err(|_| TransportError::ConnectionFailed)
    }

    #[cfg(feature = "rmw-xrce")]
    {
        // Parse locator into XRCE agent address + transport
        todo!("XRCE session creation from locator string")
    }
}
```

## Steps

### 39.1: Create `backend.rs` module with type aliases

**Files:** `packages/core/nros-c/src/backend.rs`, `packages/core/nros-c/src/lib.rs`

- [ ] Create `backend.rs` with `cfg`-gated type aliases for `Session`, `Publisher`, `Subscriber`, `ServiceServer`, `ServiceClient`
- [ ] Add `open_session()` factory function
- [ ] Add mutual exclusivity check: `#[cfg(all(feature = "rmw-zenoh", feature = "rmw-xrce"))] compile_error!(...)`
- [ ] Gate the module behind `#[cfg(any(feature = "rmw-zenoh", feature = "rmw-xrce"))]`
- [ ] `just quality` passes

### 39.2: Refactor `support.rs` to use backend types

**Files:** `packages/core/nros-c/src/support.rs`

- [ ] Replace `use nros_rmw_zenoh::ShimSession` with `use crate::backend::Session`
- [ ] Replace `ShimSession::new(...)` with `crate::backend::open_session(...)`
- [ ] Replace `get_session()` return type from `&nros_rmw_zenoh::ShimSession` to `&crate::backend::Session`
- [ ] Replace `get_session_mut()` similarly
- [ ] Replace `Box::from_raw(... as *mut ShimSession)` with `Box::from_raw(... as *mut backend::Session)`
- [ ] `just quality` passes

### 39.3: Refactor `publisher.rs` to use backend types

**Files:** `packages/core/nros-c/src/publisher.rs`

- [ ] Replace all `ShimSession` and `ShimPublisher` imports with `backend::Session` and `backend::Publisher`
- [ ] Use `nros_rmw::Publisher` trait for `publish_raw()` calls (already used, just fix casts)
- [ ] `just quality` passes

### 39.4: Refactor `subscription.rs` to use backend types

**Files:** `packages/core/nros-c/src/subscription.rs`

- [ ] Replace `ShimSession` and `ShimSubscriber` with backend types
- [ ] Use `nros_rmw::Subscriber` trait for `recv_raw()` calls
- [ ] `just quality` passes

### 39.5: Refactor `service.rs` to use backend types

**Files:** `packages/core/nros-c/src/service.rs`

- [ ] Replace `ShimSession`, `ShimServiceServer`, `ShimServiceClient` with backend types
- [ ] Use `nros_rmw::ServiceServerTrait` and `nros_rmw::ServiceClientTrait` traits for method calls
- [ ] `just quality` passes

### 39.6: Refactor `executor.rs` to use backend types

**Files:** `packages/core/nros-c/src/executor.rs`

- [ ] Replace `ShimSubscriber` and `ShimServiceServer` with backend types
- [ ] Ensure `recv_raw()` and `check_ready()` calls go through traits
- [ ] `just quality` passes

### 39.7: Gate module list on any-backend (not just zenoh)

**Files:** `packages/core/nros-c/src/lib.rs`, `packages/core/nros-c/Cargo.toml`

- [ ] Change `#[cfg(feature = "rmw-zenoh")]` on all modules to `#[cfg(any(feature = "rmw-zenoh", feature = "rmw-xrce"))]`
- [ ] Add `nros-rmw-xrce` as optional dependency in Cargo.toml
- [ ] Add `rmw-xrce = ["nros/rmw-xrce", "dep:nros-rmw-xrce"]` feature
- [ ] `just quality` passes

### 39.8: Add XRCE session creation to `open_session()`

**Files:** `packages/core/nros-c/src/backend.rs`

- [ ] Implement XRCE locator parsing (e.g., `udp/192.168.1.1:2019`)
- [ ] Create XRCE transport and session from parsed locator
- [ ] Test with native XRCE example using C API

### 39.9: C example with XRCE backend

**Files:** `examples/native/c/xrce/`

- [ ] Create `talker/` and `listener/` C examples using XRCE backend
- [ ] Update CMakeLists to support backend selection
- [ ] Verify C examples work with both zenoh and XRCE backends

### 39.10: Update C API headers and documentation

**Files:** `packages/core/nros-c/include/nros/*.h`, docs

- [ ] Update header comments to remove zenoh-specific language
- [ ] Document backend selection in C API guide
- [ ] Update `FindNanoRos.cmake` if link dependencies differ by backend

## Risk Assessment

| Risk | Mitigation |
|------|-----------|
| XRCE types may not implement all `nros-rmw` traits | Verify trait coverage in 39.7 before proceeding to 39.8 |
| XRCE session creation differs significantly from zenoh | `open_session()` encapsulates this; C API callers see no difference |
| Action support missing in XRCE | `action.rs` can remain zenoh-only for now; gate with `#[cfg(feature = "rmw-zenoh")]` until XRCE actions exist |
| Embedded no_alloc path not tested | Steps 39.1-39.6 only affect the `#[cfg(feature = "alloc")]` paths; no_alloc stub remains unchanged |

## Verification

```bash
# Zenoh backend (existing — should not regress)
cargo build -p nros-c --features "rmw-zenoh,platform-posix,ros-humble"
cargo clippy -p nros-c --features "rmw-zenoh,platform-posix,ros-humble"

# XRCE backend (new)
cargo build -p nros-c --features "rmw-xrce,platform-posix,ros-humble"
cargo clippy -p nros-c --features "rmw-xrce,platform-posix,ros-humble"

# Empty library (no backend)
cargo build -p nros-c --no-default-features

# Mutual exclusivity
cargo build -p nros-c --features "rmw-zenoh,rmw-xrce,platform-posix,ros-humble"
# ^ should fail with compile_error

# Full quality
just quality

# C tests
just test-c
```
