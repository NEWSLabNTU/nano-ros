# Phase 39: C API Multi-Backend Support

**Status: Complete (39.1–39.10)**

**Prerequisites:** Phase 34 (RMW abstraction — complete), Phase 38 (example cleanup — complete)

## Goal

Make `nros-c` support multiple RMW backends (zenoh-pico, XRCE-DDS) using the same backend-agnostic type aliases that the Rust `nros` crate provides. Today `nros-c` is hardcoded to zenoh-pico — every `_init` function imports `nros_rmw_zenoh::Shim*` types directly and casts them through `*mut c_void` pointers.

## Design

### Key insight: nros-c is a thin wrapper

`nros-c` is a C FFI wrapper around `nros`. It should **not** contain low-level backend boilerplate or directly depend on backend crates. All backend abstraction lives in the `nros` crate via `nros::internals::Rmw*` type aliases. Features simply pass through from `nros-c` to `nros`.

### Backend-agnostic type aliases in `nros::internals`

The `nros` crate provides compile-time backend selection via type aliases in `pub mod internals`:

```rust
// nros/src/lib.rs — pub mod internals

#[cfg(feature = "rmw-zenoh")]
pub type RmwSession = nros_rmw_zenoh::ShimSession;
#[cfg(feature = "rmw-zenoh")]
pub type RmwPublisher = nros_rmw_zenoh::ShimPublisher;
// ... etc.

#[cfg(feature = "rmw-xrce")]
pub type RmwSession = nros_rmw_xrce::XrceSession;
#[cfg(feature = "rmw-xrce")]
pub type RmwPublisher = nros_rmw_xrce::XrcePublisher;
// ... etc.

pub fn open_session(locator: &str, mode: SessionMode) -> Result<RmwSession, TransportError> { ... }
```

`nros-c` uses `nros::internals::RmwSession`, `nros::internals::RmwPublisher`, etc. everywhere it previously used `nros_rmw_zenoh::ShimSession`, `nros_rmw_zenoh::ShimPublisher`, etc. No `backend.rs` in `nros-c` — the abstraction lives upstream.

### Feature pass-through

`nros-c/Cargo.toml` features simply forward to `nros`:

```toml
rmw-zenoh = ["nros/rmw-zenoh"]
rmw-xrce = ["nros/rmw-xrce"]
```

No direct dependencies on `nros-rmw-zenoh` or `nros-rmw-xrce`.

### Module gating

Backend-independent modules (cdr, clock, constants, error, platform, qos) are always available. Backend-dependent modules use a `rmw_modules!` macro to reduce `#[cfg]` repetition:

```rust
macro_rules! rmw_modules {
    ($(mod $mod:ident;)*) => {
        $(
            #[cfg(any(feature = "rmw-zenoh", feature = "rmw-xrce"))]
            mod $mod;
            #[cfg(any(feature = "rmw-zenoh", feature = "rmw-xrce"))]
            pub use $mod::*;
        )*
    };
}

rmw_modules! {
    mod action;
    mod executor;
    mod guard_condition;
    mod lifecycle;
    mod node;
    mod publisher;
    mod service;
    mod subscription;
    mod support;
    mod timer;
}
```

### Feature validation

Only mutual exclusivity checks — matching the `nros` crate's pattern. Without a backend, `nros-c` compiles as a partial library (CDR, clock, error types available).

## Completed Steps

### 39.1: Add backend-agnostic type aliases to `nros::internals`

**Files:** `packages/core/nros/src/lib.rs`

- [x] Add `RmwSession`, `RmwPublisher`, `RmwSubscriber`, `RmwServiceServer`, `RmwServiceClient` type aliases with `#[cfg]` dispatch to `pub mod internals`
- [x] Add `open_session()` factory function wrapping backend-specific constructors
- [x] XRCE type aliases resolve to `nros_rmw_xrce::Xrce*` types

### 39.2: Refactor `support.rs` to use `nros::internals`

**Files:** `packages/core/nros-c/src/support.rs`

- [x] Replace `nros_rmw_zenoh::ShimSession` with `nros::internals::RmwSession`
- [x] Replace `ShimSession::new(...)` with `nros::internals::open_session(...)`
- [x] Update `get_session()` / `get_session_mut()` return types

### 39.3: Refactor `publisher.rs` to use `nros::internals`

**Files:** `packages/core/nros-c/src/publisher.rs`

- [x] Replace `ShimPublisher` casts with `nros::internals::RmwPublisher`

### 39.4: Refactor `subscription.rs` to use `nros::internals`

**Files:** `packages/core/nros-c/src/subscription.rs`

- [x] Replace `ShimSubscriber` casts with `nros::internals::RmwSubscriber`

### 39.5: Refactor `service.rs` to use `nros::internals`

**Files:** `packages/core/nros-c/src/service.rs`

- [x] Replace `ShimServiceServer` / `ShimServiceClient` casts with `nros::internals::RmwServiceServer` / `RmwServiceClient`

### 39.6: Refactor `executor.rs` to use `nros::internals`

**Files:** `packages/core/nros-c/src/executor.rs`

- [x] Replace `ShimSubscriber` / `ShimServiceServer` casts with `nros::internals::Rmw*` types

### 39.7: Clean up module gating and Cargo.toml

**Files:** `packages/core/nros-c/src/lib.rs`, `packages/core/nros-c/Cargo.toml`

- [x] Remove direct `nros-rmw-zenoh` and `nros-rmw-xrce` dependencies from Cargo.toml
- [x] Features pass through to `nros` only
- [x] Separate backend-independent modules (always compiled) from backend-dependent modules
- [x] Use `rmw_modules!` macro for backend-dependent module gating
- [x] Delete `backend.rs` — no longer needed

### 39.8: XRCE session creation in `open_session()`

**Files:** `packages/core/nros/src/lib.rs`, `packages/core/nros-c/src/support.rs`, `packages/core/nros-c/Cargo.toml`

- [x] Extended `open_session()` signature to accept `domain_id: u32` and `node_name: &str`
- [x] XRCE path: initializes transport (posix-udp or posix-serial based on feature) then calls `XrceRmw::open()` with `RmwConfig`
- [x] Zenoh path: ignores `domain_id` and `node_name` (unchanged behavior)
- [x] Added `drive_session_io()` to `nros::internals` — no-op for zenoh, calls `spin_once()` for XRCE
- [x] Updated `support_init()` to pass `domain_id` and `"nros"` node name to `open_session()`
- [x] Added backend-dependent default locator (zenoh: `tcp/127.0.0.1:7447`, XRCE: `127.0.0.1:2019`)
- [x] Added `xrce-udp` and `xrce-serial` features to `nros-c/Cargo.toml`
- [x] Called `drive_session_io()` in executor `spin_some()` to pump XRCE I/O before polling handles

### 39.9: C examples with XRCE backend

**Files:** `examples/native/c/xrce/talker/`, `examples/native/c/xrce/listener/`

- [x] Created talker and listener C examples for XRCE-DDS backend
- [x] Same C API calls as zenoh examples (backend-agnostic)
- [x] Environment variables: `XRCE_AGENT_ADDR` (default `127.0.0.1:2019`), `ROS_DOMAIN_ID`
- [x] Build: `cargo build -p nros-c --release --features "rmw-xrce,xrce-udp,platform-posix,ros-humble"`

### 39.10: Update C API headers and documentation

**Files:** `packages/core/nros-c/include/nros/init.h`, `packages/testing/nros-tests/src/fixtures/binaries.rs`

- [x] Updated header comments: "zenoh session" → "middleware session", "Zenoh locator" → "Middleware locator"
- [x] Added `build_nano_ros_c_lib_xrce()`, `build_c_xrce_example()`, builder/fixture functions for XRCE C examples to test infrastructure

## Verification

```bash
# Zenoh backend (existing — should not regress)
cargo build -p nros-c --features "rmw-zenoh,platform-posix,ros-humble"
cargo clippy -p nros-c --features "rmw-zenoh,platform-posix,ros-humble"

# XRCE backend (session creation + I/O driving functional)
cargo build -p nros-c --features "rmw-xrce,xrce-udp,platform-posix,ros-humble"

# Default features (no backend — partial library)
cargo build -p nros-c

# Mutual exclusivity (should fail with compile_error)
cargo build -p nros-c --features "rmw-zenoh,rmw-xrce,platform-posix,ros-humble"

# Full quality
just quality

# C XRCE examples (needs cmake + XRCE Agent for runtime)
cargo build -p nros-c --release --features "rmw-xrce,xrce-udp,platform-posix,ros-humble"
cd examples/native/c/xrce/talker && mkdir -p build && cd build && cmake -DNANO_ROS_ROOT=../../../../../.. .. && cmake --build .
```

## Files Modified

| File | Change |
|------|--------|
| `packages/core/nros/src/lib.rs` | Added `Rmw*` type aliases + `open_session()` + `drive_session_io()` to `pub mod internals` |
| `packages/core/nros-c/Cargo.toml` | Removed direct backend deps; added `xrce-udp`, `xrce-serial` features |
| `packages/core/nros-c/src/lib.rs` | Separated backend-independent/dependent modules; `rmw_modules!` macro |
| `packages/core/nros-c/src/support.rs` | Backend-dependent default locator; passes `domain_id`/`node_name` to `open_session()` |
| `packages/core/nros-c/src/publisher.rs` | `ShimPublisher` → `nros::internals::RmwPublisher` |
| `packages/core/nros-c/src/subscription.rs` | `ShimSubscriber` → `nros::internals::RmwSubscriber` |
| `packages/core/nros-c/src/service.rs` | `ShimServiceServer/Client` → `nros::internals::RmwServiceServer/Client` |
| `packages/core/nros-c/src/executor.rs` | `Rmw*` types + `drive_session_io()` call in `spin_some()` |
| `packages/core/nros-c/src/backend.rs` | **Deleted** — abstraction moved to `nros::internals` |
| `packages/core/nros-c/include/nros/init.h` | Removed zenoh-specific language from comments |
| `examples/native/c/xrce/talker/` | New C XRCE talker example |
| `examples/native/c/xrce/listener/` | New C XRCE listener example |
| `packages/testing/nros-tests/src/fixtures/binaries.rs` | Added XRCE C build/fixture functions |
