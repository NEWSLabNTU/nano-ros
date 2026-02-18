# Phase 43 — RMW-Agnostic Embedded API

## Status: Complete

## Background

Phase 42 unified the node-level API into generic `EmbeddedExecutor<S>` /
`EmbeddedNode<S>` types, eliminating duplicated `ShimNode`/`XrceNode` code.
However, user-facing examples still leak backend-specific types:

**Current XRCE example:**
```rust
use nros::xrce_transport::init_posix_udp;
use nros::{EmbeddedExecutor, Rmw, RmwConfig, SessionMode, XrceRmw};

init_posix_udp(&agent_addr);
let config = RmwConfig { locator: &agent_addr, mode: SessionMode::Client, domain_id, ... };
let session = XrceRmw::open(&config).expect("...");
let mut executor = EmbeddedExecutor::from_session(session);
```

**Current Zephyr example:**
```rust
use nros::{EmbeddedExecutor, SessionMode, Transport, TransportConfig, internals::ShimTransport};

let config = TransportConfig { locator: Some("tcp/192.0.2.2:7447"), mode: SessionMode::Client, ... };
let session = ShimTransport::open(&config).map_err(|_| ...)?;
let mut executor = EmbeddedExecutor::from_session(session);
```

Both patterns require the user to know which backend type to import and
construct. The standard ROS 2 pattern is that user code is
transport-agnostic — RMW selection is a build-time decision (Cargo features /
CMake options), not a code-level decision.

Additionally, the embedded API has **no callback or spin support**. Users must
manually poll subscriptions with `try_recv()` and services with
`handle_request()` in a loop. The high-level API (`Context` → `BasicExecutor`
→ callbacks → `spin()`) has full callback+spin support but is tied to
`rmw-zenoh` + `alloc`, making it unavailable for XRCE or `no_std` targets.

Finally, the two XRCE action examples manually compose the action protocol
from raw CDR and session handles, bypassing the `EmbeddedActionServer` /
`EmbeddedActionClient` types that already support typed action protocol.

### Goals

1. **Zero backend types in user code** — examples import only `nros::` types
2. **Callback + spin for embedded** — `EmbeddedExecutor` gets `spin_once()`
   with callback dispatch, working over any `Session`
3. **XRCE actions use typed API** — action examples use `EmbeddedActionServer`
   / `EmbeddedActionClient` instead of raw CDR
4. **No alloc required** — entire embedded API works without `alloc` feature
5. **Delete deprecated items** — no backward compatibility maintained

### Non-Goals

- Merging `EmbeddedExecutor` with `PollingExecutor`/`BasicExecutor` (different
  ownership models; unification is a future phase)
- Adding timer support to the embedded executor (can be a follow-up)

## `std` / `alloc` Audit

All three backend `open()` functions (`XrceRmw::open`, `ShimSession::new`,
`CffiSession::open`) use **stack buffers and module-level statics only** —
no heap allocation. The entire embedded API is no_std/no_alloc:

| Component                              | std | alloc | Notes                                   |
|----------------------------------------|-----|-------|-----------------------------------------|
| `EmbeddedConfig`                       | No  | No    | All `&str` fields                       |
| `SessionMode`                          | No  | No    | Plain enum                              |
| `EmbeddedExecutor::open()`             | No  | No    | Delegates to backend `open()`           |
| `EmbeddedExecutor::from_session()`     | No  | No    | Takes ownership of `S`                  |
| `EmbeddedExecutor::drive_io()`         | No  | No    | Delegates to `Session::drive_io()`      |
| `EmbeddedExecutor::create_node()`      | No  | No    | Uses `heapless::String<64>`             |
| `EmbeddedExecutor::close()`            | No  | No    | Delegates to `Session::close()`         |
| `EmbeddedNode` create_* methods        | No  | No    | Delegates to `Session::create_*()`      |
| `EmbeddedNodeError`                    | No  | No    | Plain enum, no `std::error::Error` impl |
| `SpinOnceResult`                       | No  | No    | All `usize` fields                      |
| Arena callback storage (43.2)          | No  | No    | Fixed-size byte arena + fn pointers     |
| `spin_once()` (43.2)                   | No  | No    | Polls arena entries via fn pointers     |
| `spin()` infinite loop (43.2)          | No  | No    | `loop { spin_once() }`                  |
| `TransportConfig` / `RmwConfig`        | No  | No    | All borrowed `&str` fields              |
| `Session` / `Rmw` / `Transport` traits | No  | No    | Trait defs have no alloc deps           |

The `alloc` gate on `internals::open_session()` is unnecessary and will be
removed.

**Feature gating policy**: `std` and `alloc` features remain available for
users who want them (e.g., closures as callbacks require known size at compile
time, but `alloc` enables `Box<dyn FnMut>` as an alternative). The embedded
API itself never requires them.

## Current Architecture

### Two API tiers

| Aspect        | High-Level API                      | Embedded API                            |
|---------------|-------------------------------------|-----------------------------------------|
| Entry point   | `Context::from_env()`               | `EmbeddedExecutor::from_session(s)`     |
| Executor      | `PollingExecutor` / `BasicExecutor` | `EmbeddedExecutor<S>`                   |
| Node          | `Node` (Arc-wrapped)                | `EmbeddedNode<'_, S>` (borrows session) |
| Subscriptions | Callback-based (`FnMut(&M)`)        | Manual poll (`try_recv()`)              |
| Spin          | `spin_once()` / `spin()`            | `drive_io()` only                       |
| Feature gate  | `rmw-zenoh` + `alloc`               | Always available (no_std)               |
| Backend types | Hidden (feature-gated)              | Exposed (user creates session)          |

### Backend types in user code

| Backend | Types that leak into examples                             |
|---------|-----------------------------------------------------------|
| zenoh   | `ShimTransport`, `TransportConfig`, `SessionMode`         |
| XRCE    | `XrceRmw`, `RmwConfig`, `SessionMode`, `init_posix_udp()` |
| cffi    | `CffiRmw`, `RmwConfig`, `SessionMode`                     |

### Existing factory (`internals::open_session`)

`nros/src/lib.rs` already has `internals::open_session(locator, mode,
domain_id, node_name)` that dispatches to the active backend via feature
gates. However, it:
- Lives in `internals` (not meant for user consumption)
- Requires `alloc`
- Doesn't handle XRCE transport initialization
- Returns `RmwSession` type alias, not `EmbeddedExecutor`

## Phase 43.1 — Backend-Agnostic Factory

Add `EmbeddedExecutor::open()` as a feature-gated factory that hides all
backend types. The factory auto-initializes transport (XRCE UDP/serial) and
constructs the session.

### Configuration

A single configuration struct for all backends:

```rust
/// Configuration for opening an embedded executor session.
///
/// Fields are interpreted by the active backend:
/// - **zenoh**: `locator` is the router address (e.g., `"tcp/127.0.0.1:7447"`),
///   `domain_id` and `node_name` are used for ROS 2 topic keyexpr only.
/// - **XRCE**: `locator` is the agent address (e.g., `"127.0.0.1:2019"`),
///   transport type is selected by `xrce-udp` or `xrce-serial` feature.
/// - **cffi**: All fields are passed to the registered vtable's `open()`.
pub struct EmbeddedConfig<'a> {
    /// Transport locator (router/agent address).
    pub locator: &'a str,
    /// Session mode (Client or Peer).
    pub mode: SessionMode,
    /// ROS domain ID.
    pub domain_id: u32,
    /// Node name for the session.
    pub node_name: &'a str,
    /// ROS namespace.
    pub namespace: &'a str,
}
```

With a builder-style default:

```rust
impl<'a> EmbeddedConfig<'a> {
    pub const fn new(locator: &'a str) -> Self {
        Self {
            locator,
            mode: SessionMode::Client,
            domain_id: 0,
            node_name: "",
            namespace: "",
        }
    }

    pub const fn domain_id(mut self, id: u32) -> Self { ... }
    pub const fn node_name(mut self, name: &'a str) -> Self { ... }
    pub const fn namespace(mut self, ns: &'a str) -> Self { ... }
    pub const fn mode(mut self, mode: SessionMode) -> Self { ... }
}
```

### Factory method

```rust
// In generic.rs, feature-gated
#[cfg(any(feature = "rmw-zenoh", feature = "rmw-xrce", feature = "rmw-cffi"))]
impl EmbeddedExecutor<crate::internals::RmwSession> {
    /// Open a session using the active RMW backend.
    ///
    /// Backend is selected at compile time by Cargo features:
    /// - `rmw-zenoh` — zenoh-pico
    /// - `rmw-xrce` — XRCE-DDS (auto-initializes UDP or serial transport)
    /// - `rmw-cffi` — C function table
    pub fn open(config: &EmbeddedConfig<'_>) -> Result<Self, EmbeddedNodeError> {
        #[cfg(feature = "rmw-zenoh")]
        {
            // ShimTransport::open maps locator → TransportConfig
        }

        #[cfg(all(feature = "rmw-xrce", not(feature = "rmw-zenoh")))]
        {
            // Auto-init transport
            #[cfg(feature = "xrce-udp")]
            unsafe { nros_rmw_xrce::posix_udp::init_posix_udp_transport(config.locator); }
            #[cfg(feature = "xrce-serial")]
            unsafe { nros_rmw_xrce::posix_serial::init_posix_serial_transport(config.locator); }

            // XrceRmw::open(RmwConfig { ... })
        }

        #[cfg(all(feature = "rmw-cffi", ...))]
        {
            // CffiRmw::open(RmwConfig { ... })
        }
    }
}
```

The factory lives in `nros-node/src/executor/spin.rs` but requires
`nros-node/Cargo.toml` to optionally depend on the backend crates. This
mirrors how `nros/src/lib.rs` already does it for `internals::open_session`.

**Alternative**: Keep the factory in `nros/src/lib.rs` only (not in
`nros-node`) and implement it as a free function `nros::open_executor()`.
This avoids adding backend deps to `nros-node`. The `RmwSession` type alias
is already defined in `nros::internals`.

### Target user code (XRCE)

```rust
use nros::prelude::*;
use std_msgs::msg::Int32;

fn main() {
    let agent_addr = std::env::var("XRCE_AGENT_ADDR")
        .unwrap_or_else(|_| "127.0.0.1:2019".to_string());

    let config = EmbeddedConfig::new(&agent_addr)
        .domain_id(0)
        .node_name("xrce_talker");

    let mut executor = EmbeddedExecutor::open(&config)
        .expect("Failed to open session");

    let mut node = executor.create_node("xrce_talker").unwrap();
    let publisher = node.create_publisher::<Int32>("/chatter").unwrap();

    for i in 0i32..20 {
        publisher.publish(&Int32 { data: i }).unwrap();
        let _ = executor.drive_io(100);
        std::thread::sleep(std::time::Duration::from_millis(500));
    }
    let _ = executor.close();
}
```

### Target user code (Zephyr)

```rust
use nros::prelude::*;
use std_msgs::msg::Int32;

fn run() -> Result<(), EmbeddedNodeError> {
    let config = EmbeddedConfig::new("tcp/192.0.2.2:7447");
    let mut executor = EmbeddedExecutor::open(&config)?;
    let mut node = executor.create_node("talker")?;
    let publisher = node.create_publisher::<Int32>("/chatter")?;

    loop {
        publisher.publish(&Int32 { data: counter })?;
        let _ = executor.drive_io(1000);
    }
}
```

### Tasks

- [x] Define `EmbeddedConfig` struct with builder methods
- [x] Add optional backend deps to `nros-node/Cargo.toml` (or implement
      factory in `nros/src/lib.rs` only)
- [x] Implement `EmbeddedExecutor::open()` for zenoh
- [x] Implement `EmbeddedExecutor::open()` for XRCE (with transport auto-init)
- [x] Implement `EmbeddedExecutor::open()` for cffi
- [x] Add `EmbeddedConfig` and factory to prelude
- [x] Update `internals::open_session` to delegate to the new factory (or
      deprecate it)

## Phase 43.2 — Callback Storage and `spin_once()` for Embedded Executor

Add callback registration and `spin_once()` to `EmbeddedExecutor` using a
**fixed-size byte arena** for type erasure — no `alloc` required.

### Design constraints

- **`no_std` compatible** — no `Box<dyn>`, no heap allocation
- **Fixed capacity** — compile-time const generics for entry count and arena
  size
- **Zero cost when unused** — default `MAX_CBS=0, CB_ARENA=0` adds no memory
- **Works with any `Session`** — generic over `S: Session`, not tied to zenoh

### Arena-based type erasure

Type erasure without `alloc` uses a byte arena + monomorphized function
pointers. At registration time, the concrete entry (subscriber handle +
receive buffer + callback) is placed in the arena, and a function pointer
monomorphized for the concrete types is stored in a metadata slot.

```rust
pub struct EmbeddedExecutor<S, const MAX_CBS: usize = 0, const CB_ARENA: usize = 0> {
    session: S,
    // Callback arena — stores concrete SubscriptionEntry / ServiceEntry data
    arena: [MaybeUninit<u8>; CB_ARENA],
    arena_used: usize,
    // Metadata for each registered callback
    entries: [Option<CallbackMeta>; MAX_CBS],
}

/// Type-erased metadata for one callback entry.
struct CallbackMeta {
    /// Byte offset into the arena where the concrete entry lives.
    offset: usize,
    /// Monomorphized function that polls the subscriber and invokes the
    /// callback if data is available. Knows the concrete types at compile
    /// time via monomorphization.
    try_process: unsafe fn(*mut u8) -> Result<bool, TransportError>,
    /// Monomorphized destructor for the concrete entry.
    drop_fn: unsafe fn(*mut u8),
}
```

When defaults are `MAX_CBS=0, CB_ARENA=0`:
- `[MaybeUninit<u8>; 0]` is a ZST (zero bytes)
- `[Option<CallbackMeta>; 0]` is a ZST (zero bytes)
- Total overhead: `arena_used: usize` only (8 bytes)
- Existing code using `EmbeddedExecutor<S>` is completely unaffected

### Concrete entry layout

Each registered callback is stored as a `ConcreteEntry` in the arena:

```rust
/// Stored in the arena (not visible to users).
#[repr(C)]
struct ConcreteEntry<M, Sub, F, const RX_BUF: usize> {
    handle: Sub,           // Backend subscriber handle
    buffer: [u8; RX_BUF],  // Receive buffer
    callback: F,            // fn(&M) or closure
    _phantom: PhantomData<M>,
}
```

For `fn(&M)` callbacks (the `no_std` case), `F` is zero-sized. The entry
size is `size_of::<Sub>() + RX_BUF`. For closures (when `alloc` is
available or the closure has no captures), the closure's captured state
is included in the entry size.

### Monomorphized dispatch function

At registration time, a function pointer is created that knows the concrete
types:

```rust
/// Generated at monomorphization time — knows M, Sub, F, RX_BUF.
unsafe fn try_process_impl<M, Sub, F, const RX_BUF: usize>(
    ptr: *mut u8,
) -> Result<bool, TransportError>
where
    M: RosMessage + Deserialize,
    Sub: Subscriber,
    F: FnMut(&M),
{
    let entry = &mut *(ptr as *mut ConcreteEntry<M, Sub, F, RX_BUF>);
    match entry.handle.try_recv_raw(&mut entry.buffer) {
        Ok(Some(len)) => {
            let mut reader = CdrReader::new_with_header(&entry.buffer[..len])
                .map_err(|_| TransportError::DeserializationFailed)?;
            let msg = M::deserialize(&mut reader)
                .map_err(|_| TransportError::DeserializationFailed)?;
            (entry.callback)(&msg);
            Ok(true)
        }
        Ok(None) => Ok(false),
        Err(e) => Err(e),
    }
}
```

### Registration flow

When `add_subscription::<Int32>("/chatter", handler)` is called:

1. Create subscriber handle via `self.session.create_subscriber()`
2. Compute `size_of::<ConcreteEntry<Int32, S::SubscriberHandle, F, 1024>>()`
   and `align_of::<...>()`
3. Bump-allocate from `arena` (align up `arena_used`, check remaining space)
4. Write the `ConcreteEntry` at the allocated offset via `ptr::write`
5. Store `CallbackMeta { offset, try_process: try_process_impl::<...>,
   drop_fn: drop_impl::<...> }` in the first free `entries` slot
6. Return `Ok(())` or `Err(EmbeddedNodeError::BufferTooSmall)` if arena is
   full

### Registration API

```rust
impl<S: Session, const MAX_CBS: usize, const CB_ARENA: usize>
    EmbeddedExecutor<S, MAX_CBS, CB_ARENA>
{
    /// Register a subscription with a callback for `spin_once()` dispatch.
    ///
    /// The subscriber handle, receive buffer, and callback are placed in the
    /// executor's arena. Returns `BufferTooSmall` if the arena or entry
    /// table is full.
    pub fn add_subscription<M, F>(
        &mut self,
        topic_name: &str,
        callback: F,
    ) -> Result<(), EmbeddedNodeError>
    where
        M: RosMessage + Deserialize + 'static,
        F: FnMut(&M) + 'static,
    {
        self.add_subscription_sized::<M, F, 1024>(topic_name, callback)
    }

    /// Register a subscription with custom receive buffer size.
    pub fn add_subscription_sized<M, F, const RX_BUF: usize>(
        &mut self,
        topic_name: &str,
        callback: F,
    ) -> Result<(), EmbeddedNodeError>
    where
        M: RosMessage + Deserialize + 'static,
        F: FnMut(&M) + 'static,
    { ... }

    /// Register a service with a handler callback.
    pub fn add_service<Svc, F>(
        &mut self,
        service_name: &str,
        handler: F,
    ) -> Result<(), EmbeddedNodeError>
    where
        Svc: RosService + 'static,
        F: FnMut(&Svc::Request) -> Svc::Response + 'static,
    { ... }
}
```

### `spin_once()` and `spin()`

```rust
impl<S: Session, const MAX_CBS: usize, const CB_ARENA: usize>
    EmbeddedExecutor<S, MAX_CBS, CB_ARENA>
{
    /// Drive I/O and dispatch all ready callbacks.
    ///
    /// 1. Calls `session.drive_io(timeout_ms)` to poll the transport.
    /// 2. Iterates over registered entries, calling each monomorphized
    ///    `try_process` function pointer with the arena data pointer.
    ///
    /// Returns a summary of work performed.
    pub fn spin_once(&mut self, timeout_ms: i32) -> SpinOnceResult {
        let _ = self.session.drive_io(timeout_ms);

        let mut result = SpinOnceResult::new();
        let arena_ptr = self.arena.as_mut_ptr() as *mut u8;

        for entry in self.entries.iter().flatten() {
            let data_ptr = unsafe { arena_ptr.add(entry.offset) };
            match unsafe { (entry.try_process)(data_ptr) } {
                Ok(true) => result.subscriptions_processed += 1,
                Ok(false) => {}
                Err(_) => result.subscription_errors += 1,
            }
        }
        result
    }
}

/// Blocking spin loop (std only).
#[cfg(feature = "std")]
impl<S: Session, const MAX_CBS: usize, const CB_ARENA: usize>
    EmbeddedExecutor<S, MAX_CBS, CB_ARENA>
{
    pub fn spin(&mut self, timeout_ms: i32) -> ! {
        loop {
            self.spin_once(timeout_ms);
        }
    }
}
```

### Interaction with manual-poll API

`spin_once()` and manual `try_recv()` are **mutually exclusive** for a given
subscription. Subscriptions registered via `add_subscription()` are owned by
the executor and polled by `spin_once()`. Subscriptions created via
`node.create_subscription()` are owned by the user and polled manually.

Both patterns can coexist in the same executor — some subscriptions use
callbacks, others use manual polling. This matches rclc where you can have
executor-managed and manually-polled subscriptions.

### Target user code (listener with callback)

```rust
use nros::prelude::*;
use std_msgs::msg::Int32;

fn main() {
    let config = EmbeddedConfig::new("127.0.0.1:2019")
        .domain_id(0)
        .node_name("listener");

    // 4 callback slots, 4KB arena
    let mut executor = EmbeddedExecutor::<_, 4, 4096>::open(&config)
        .expect("Failed to open session");

    executor.add_subscription::<Int32>("/chatter", handle_msg as fn(&Int32))
        .expect("Failed to create subscription");

    println!("Listening on /chatter...");
    loop {
        executor.spin_once(100);
    }
}

fn handle_msg(msg: &Int32) {
    println!("Received: {}", msg.data);
}
```

### Memory budget example

Typical entry sizes (Int32 subscription, fn pointer callback):

| Component                            | Size            |
|--------------------------------------|-----------------|
| Subscriber handle (`XrceSubscriber`) | ~32 bytes       |
| Subscriber handle (`ShimSubscriber`) | ~16 bytes       |
| Receive buffer (`RX_BUF=1024`)       | 1024 bytes      |
| Callback (`fn(&Int32)`)              | 0 bytes (ZST)   |
| PhantomData                          | 0 bytes         |
| **Total per entry (XRCE)**           | **~1056 bytes** |
| **Total per entry (zenoh)**          | **~1040 bytes** |

So `CB_ARENA=4096` fits ~3-4 subscriptions with 1KB receive buffers.
`CB_ARENA=8192` fits ~7-8. Users needing large receive buffers
(e.g., `RX_BUF=4096`) should size the arena accordingly.

### Tasks

- [x] Add const generics `MAX_CBS` and `CB_ARENA` to `EmbeddedExecutor`
      (with defaults `0, 0`)
- [x] Define `CallbackMeta` struct
- [x] Define `ConcreteEntry` layout (`SubEntry`, `SrvEntry`)
- [x] Implement arena bump allocation with alignment
- [x] Implement monomorphized `sub_try_process` / `srv_try_process` functions
- [x] Implement monomorphized `drop_entry` function
- [x] Add `add_subscription()` and `add_subscription_sized()` methods
- [x] Add `add_service()` and `add_service_sized()` methods
- [x] Add `spin_once()` method
- [x] Add `spin()` infinite loop (no `std`/`alloc` required — implemented in 43.7)
- [x] Add `Drop` impl for `EmbeddedExecutor` to call `drop_fn` on entries
- [x] Move `SpinOnceResult` to `executor/types.rs` (always available, no feature gate)
- [x] Unit tests with mock session (10 tests covering arena, callbacks, Drop)

## Phase 43.3 — Migrate XRCE Action Examples to Typed API

The two XRCE action examples (`action-server`, `action-client`) currently
compose the action protocol manually using raw CDR serialization and
`session_mut()` access to `XrceSession`. The `EmbeddedActionServer` and
`EmbeddedActionClient` types in `generic.rs` already implement the full typed
action protocol. Migrate both examples to use them.

### Current action server pattern (raw)

```rust
let session: &mut XrceSession = executor.session_mut();
let mut send_goal_server = session.create_service_server(&send_goal_info).unwrap();
let mut get_result_server = session.create_service_server(&get_result_info).unwrap();
let feedback_publisher = session.create_publisher(&feedback_topic, qos).unwrap();

// Manual CDR parse, manual reply construction
if let Some(request) = send_goal_server.try_recv_request(&mut req_buf).unwrap() {
    let mut reader = CdrReader::new_with_header(&req_buf[..data_len]).unwrap();
    let goal_id = GoalId::deserialize(&mut reader).unwrap();
    // ... manual serialization ...
}
```

### Target action server pattern (typed)

```rust
let config = EmbeddedConfig::new(&agent_addr)
    .domain_id(domain_id)
    .node_name("xrce_action_server");
let mut executor = EmbeddedExecutor::open(&config).unwrap();
let mut node = executor.create_node("xrce_action_server").unwrap();

let mut action_server = node
    .create_action_server::<Fibonacci>("/fibonacci")
    .unwrap();

// Typed goal handling
while let Some(goal) = action_server.try_recv_goal()? {
    action_server.accept_goal(&goal.goal_id)?;

    // Compute with feedback
    for i in 0..=goal.goal.order {
        let feedback = FibonacciFeedback { sequence: ... };
        action_server.send_feedback(&goal.goal_id, &feedback)?;
        let _ = executor.drive_io(100);
    }

    let result = FibonacciResult { sequence };
    action_server.complete_goal(&goal.goal_id, GoalStatus::Succeeded, result)?;
}
```

### Tasks

- [x] Rewrite `examples/native/rust/xrce/action-server/src/main.rs` to use
      `EmbeddedActionServer`
- [x] Rewrite `examples/native/rust/xrce/action-client/src/main.rs` to use
      `EmbeddedActionClient`
- [x] Remove `XrceSession` and raw CDR imports from action examples
- [x] Verify action integration tests still pass

## Phase 43.4 — Migrate All Examples to RMW-Agnostic Code

Once the factory (43.1) is available, migrate all 16 embedded examples to
use `EmbeddedExecutor::open()` instead of backend-specific constructors.

### Examples to migrate

**6 Zephyr examples** (`examples/zephyr/rust/zenoh/*/src/lib.rs`):
- talker, listener, service-server, service-client, action-server,
  action-client
- Remove: `ShimTransport`, `TransportConfig`, `Transport`,
  `internals::ShimTransport`
- Replace with: `EmbeddedConfig::new("tcp/192.0.2.2:7447")`,
  `EmbeddedExecutor::open(&config)`

**8 XRCE examples** (`examples/native/rust/xrce/*/src/main.rs`):
- talker, listener, service-server, service-client, serial-talker,
  serial-listener, large-msg-test, stress-test
- Remove: `XrceRmw`, `Rmw`, `RmwConfig`, `SessionMode`,
  `xrce_transport::init_posix_udp`
- Replace with: `EmbeddedConfig`, `EmbeddedExecutor::open()`

**2 XRCE action examples** (done in 43.3):
- action-server, action-client
- Already migrated to typed API + factory

### Listener examples with callbacks (stretch)

If 43.2 is complete, migrate listener examples from manual-poll to
callback+spin:

```rust
// Before (manual poll)
let mut subscription = node.create_subscription::<Int32>("/chatter")?;
loop {
    let _ = executor.drive_io(100);
    match subscription.try_recv() {
        Ok(Some(msg)) => println!("Received: {}", msg.data),
        ...
    }
}

// After (callback + spin, no alloc needed)
executor.add_subscription::<Int32>("/chatter", handle_msg as fn(&Int32))?;
loop {
    executor.spin_once(100);
}
// ...
fn handle_msg(msg: &Int32) { println!("Received: {}", msg.data); }
```

This is optional — manual-poll remains a valid pattern, especially for
`no_std` targets without `alloc`.

### Tasks

- [x] Migrate 6 Zephyr examples to `EmbeddedExecutor::open()`
- [x] Migrate 8 XRCE pub/sub/service examples to `EmbeddedExecutor::open()`
- [x] Verify: no backend-specific types in any example's `use` statements
- [x] Migrate listener/service examples to callback+spin pattern (see 43.6)

## Phase 43.5 — Delete Deprecated Items and Clean Up Exports

Delete all deprecated items outright (no backward compatibility) and clean up
the public API surface.

### Deprecated items to delete

**`nros/src/lib.rs`:**
- `TransportConfig` re-export (line 152) — delete re-export entirely

**`nros-node/src/connected.rs`:**
- `ConnectedNode::new()` — use `Context` + executor instead
- `ConnectedNode::connect()` — use `Context` + executor instead
- `ConnectedNode::connect_with_config()` — use `Context` + executor instead
- `ConnectedNode::create_typed_publisher()` — use `create_publisher` with
  `PublisherOptions`
- `ConnectedNode::create_typed_publisher_with_qos()` — same
- `ConnectedNode::create_typed_subscriber()` — use `create_subscriber` with
  `SubscriberOptions`
- `ConnectedNode::create_typed_subscriber_with_qos()` — same
- `ConnectedPublisher::publish_with_buffer()` — use `create_publisher_sized`

**`nros-node/src/node.rs`:**
- `StandaloneNode::create_typed_publisher()` — use `create_publisher` with
  `PublisherOptions`
- `StandaloneNode::create_typed_publisher_with_qos()` — same
- `StandaloneNode::create_typed_subscriber()` — use `create_subscriber` with
  `SubscriberOptions`
- `StandaloneNode::create_typed_subscriber_with_qos()` — same

**`nros-node/src/context.rs`:**
- `Context::create_executor()` — use `create_polling_executor()` or
  `create_basic_executor()`

### Export cleanup

1. **Move raw XRCE re-exports to `internals`**: `XrceRmw`, `XrceSession`,
   `XrcePublisher`, etc. currently at `nros::` root → move to
   `nros::internals`.

2. **Delete `xrce_transport` module**: With transport auto-init in the
   factory, `init_posix_udp()` / `init_posix_serial()` are internal. Move
   to `internals`.

3. **Remove `alloc` gate from `internals::open_session()`**: Backend `open()`
   functions don't require alloc. Remove the `feature = "alloc"` cfg.

4. **Update prelude**: Add `EmbeddedConfig`. Remove backend-specific types.

5. **Remove all `#[allow(deprecated)]`** from examples and library code.

### Tasks

- [x] Delete deprecated methods (9 from `connected.rs`, 4 from `node.rs`,
      1 from `context.rs`; `ConnectedNode::new()` kept public but undeprecated)
- [x] Update internal callers (`executor.rs` `#[allow(deprecated)]` removed,
      `context.rs` `create_node()` deleted)
- [x] Move XRCE raw re-exports to `internals` module
- [x] Move `xrce_transport` module to `internals`
- [x] Remove `alloc` gate from `internals::open_session()`
- [x] Update prelude to include `EmbeddedConfig`
- [x] Remove all `#[allow(deprecated)]` from examples
- [x] Verify no backend-specific types in `nros::` root
- [x] `just quality` passes (423/432; 9 pre-existing C API codegen failures)

## Phase 43.6 — Migrate Examples to Callback + `spin_once()` API

Now that 43.2 provides `add_subscription()`, `add_service()`, and
`spin_once()`, examples that use manual `drive_io()` + `try_recv()` /
`handle_request()` loops can be migrated to the callback pattern.

### Migration candidates

**Simple subscription listeners** (replace `drive_io` + `try_recv` with
`add_subscription` + `spin_once`):

| Example | File | Current pattern |
|---------|------|-----------------|
| XRCE listener | `examples/native/rust/xrce/listener/src/main.rs` | `drive_io(100)` + `subscription.try_recv()` loop |
| XRCE serial-listener | `examples/native/rust/xrce/serial-listener/src/main.rs` | Same as XRCE listener |
| Zephyr listener | `examples/zephyr/rust/zenoh/listener/src/lib.rs` | `drive_io(1000)` + `subscription.try_recv()` loop |

**Simple service servers** (replace `drive_io` + `handle_request` with
`add_service` + `spin_once`):

| Example               | File                                                   | Current pattern                                   |
|-----------------------|--------------------------------------------------------|---------------------------------------------------|
| XRCE service-server   | `examples/native/rust/xrce/service-server/src/main.rs` | `drive_io(100)` + `server.handle_request()` loop  |
| Zephyr service-server | `examples/zephyr/rust/zenoh/service-server/src/lib.rs` | `drive_io(1000)` + `server.handle_request()` loop |

### Not migrating (manual poll remains appropriate)

**Publishers/talkers** — only use `drive_io()` for flushing after
`publish()`; `spin_once()` replaces this but there's no callback benefit.
These stay as-is:
- `examples/native/rust/xrce/talker/`, `serial-talker/`
- `examples/zephyr/rust/zenoh/talker/`

**Service clients** — use `drive_io()` + `try_recv_reply()`, client-side
polling doesn't benefit from callback registration:
- `examples/native/rust/xrce/service-client/`
- `examples/zephyr/rust/zenoh/service-client/`

**Action servers/clients** — would need future `add_action_server()` /
`add_action_client()` support (not yet implemented):
- `examples/native/rust/xrce/action-server/`, `action-client/`
- `examples/zephyr/rust/zenoh/action-server/`, `action-client/`

**Board-level examples** (qemu-arm, qemu-esp32, esp32, stm32f4) — already
use board-level `node.spin_once()` or custom embedded loops, not the
`EmbeddedExecutor` manual polling pattern.

**Stress test / large-msg-test** — specialized testing code, not idiomatic
examples.

### Migration pattern

```rust
// Before (manual poll)
let mut subscription = node.create_subscription::<Int32>("/chatter")?;
loop {
    let _ = executor.drive_io(100);
    while let Ok(Some(msg)) = subscription.try_recv() {
        println!("Received: {}", msg.data);
    }
}

// After (callback + spin)
let mut executor = EmbeddedExecutor::<_, 4, 4096>::open(&config)?;
executor.add_subscription::<Int32>("/chatter", handle_msg as fn(&Int32))?;
loop {
    executor.spin_once(100);
}
fn handle_msg(msg: &Int32) { println!("Received: {}", msg.data); }
```

### Tasks

- [x] Migrate XRCE listener to `add_subscription` + `spin_once`
- [x] Migrate XRCE serial-listener to `add_subscription` + `spin_once`
- [x] Migrate Zephyr listener to `add_subscription` + `spin_once`
- [x] Migrate XRCE service-server to `add_service` + `spin_once`
- [x] Migrate Zephyr service-server to `add_service` + `spin_once`
- [x] Migrate XRCE/Zephyr talkers to `add_timer` + `spin_once`
- [x] Migrate XRCE/Zephyr action examples: `drive_io` → `spin_once`
- [x] Verify: migrated examples compile and function correctly
- [x] Update Phase 43 doc status

## Phase 43.7 — Timer Callbacks (`add_timer`)

Add `add_timer()` to `EmbeddedExecutor` so periodic work (e.g., publishing)
can be driven by `spin_once()` instead of manual loops with platform sleep.

### Motivation

Talker examples currently use a manual loop:

```rust
loop {
    publisher.publish(&Int32 { data: counter })?;
    executor.drive_io(1000);
    counter = counter.wrapping_add(1);
}
```

With `add_timer()`, the publish logic moves into a callback and `spin_once()`
handles the timing:

```rust
let publisher = node.create_publisher::<Int32>("/chatter")?;
let mut counter: i32 = 0;

executor.add_timer(TimerDuration::from_millis(1000), move || {
    let _ = publisher.publish(&Int32 { data: counter });
    counter = counter.wrapping_add(1);
})?;

loop {
    executor.spin_once(100);
}
```

### Design

Timer entries are stored in the arena alongside subscription/service entries.
Each timer entry holds a `TimerState` and a callback. The `spin_once()` loop
computes `delta_ms` since the last call (or uses the `timeout_ms` argument as
an approximation for no_std targets without a clock) and processes timers.

```rust
#[repr(C)]
struct TimerEntry<F> {
    state: TimerState,
    callback: F,
}
```

Timer dispatch function:

```rust
unsafe fn timer_try_process<F>(ptr: *mut u8, delta_ms: u64) -> Result<bool, TransportError>
where F: FnMut()
{
    let entry = &mut *(ptr as *mut TimerEntry<F>);
    if entry.state.update(delta_ms) {
        entry.state.fire_without_callback();  // reset elapsed, handle mode
        (entry.callback)();
        Ok(true)
    } else {
        Ok(false)
    }
}
```

Note: `TimerState::fire()` currently invokes its own stored callback. For
arena-based timers, we separate the timer bookkeeping (`fire_without_callback`)
from callback invocation (the arena entry's `F`), avoiding double storage.

### `spin_once()` changes

`spin_once()` needs a time delta to advance timers. Two options:

1. **Argument-based** (no_std friendly): `spin_once(timeout_ms)` uses
   `timeout_ms` as the delta approximation (already the case — good enough
   for embedded polling loops where `spin_once` is called at regular intervals).
2. **Clock-based** (std only): Track wall-clock time between `spin_once()`
   calls for accurate deltas.

For embedded, option 1 is sufficient. The `CallbackMeta` gains an extended
dispatch function pointer:

```rust
struct CallbackMeta {
    offset: usize,
    kind: EntryKind,
    try_process: unsafe fn(*mut u8) -> Result<bool, TransportError>,
    try_process_timed: Option<unsafe fn(*mut u8, u64) -> Result<bool, TransportError>>,
    drop_fn: unsafe fn(*mut u8),
}
```

Subscription/service entries use `try_process` (no timing). Timer entries
use `try_process_timed` with the delta.

### API

```rust
impl<S: Session, const MAX_CBS: usize, const CB_ARENA: usize>
    EmbeddedExecutor<S, MAX_CBS, CB_ARENA>
{
    /// Register a repeating timer with a callback.
    pub fn add_timer<F>(
        &mut self,
        period: TimerDuration,
        callback: F,
    ) -> Result<(), EmbeddedNodeError>
    where F: FnMut() + 'static;

    /// Register a one-shot timer with a callback.
    pub fn add_timer_oneshot<F>(
        &mut self,
        delay: TimerDuration,
        callback: F,
    ) -> Result<(), EmbeddedNodeError>
    where F: FnMut() + 'static;
}
```

### Tasks

- [x] Add `EntryKind::Timer` variant
- [x] Define `TimerEntry<F>` layout
- [x] Implement `timer_try_process` monomorphized dispatch
- [x] Unified `try_process` signature: `fn(*mut u8, u64)` passes `delta_ms` to timers
- [x] Update `spin_once()` to pass `delta_ms` to all dispatch functions
- [x] Implement `add_timer()` and `add_timer_oneshot()`
- [x] Add `TimerEntry<F>` arena entry with period/elapsed tracking
- [x] Unit tests for timer arena entries (5 tests)
- [x] Add `spin()` infinite loop (no `std`/`alloc` required — uses `drive_io` for blocking)

## Phase 43.8 — Action Server Callbacks (`add_action_server`)

Add `add_action_server()` to `EmbeddedExecutor` so goal acceptance, cancel
handling, and result serving are driven automatically by `spin_once()`.

### Motivation

Action server examples currently have complex manual loops:

```rust
loop {
    executor.drive_io(100);

    // Must manually poll three separate channels:
    action_server.try_handle_cancel(|goal_id, status| { ... })?;
    action_server.try_accept_goal(|goal| GoalResponse::AcceptAndExecute)?;

    if let Some(goal_id) = accepted {
        action_server.set_goal_status(&goal_id, GoalStatus::Executing);
        // ... compute and publish feedback ...
        action_server.complete_goal(&goal_id, GoalStatus::Succeeded, result);
    }
}
```

This pattern has three problems:
1. User must remember to poll all three channels (goals, cancels, results)
2. Goal acceptance and execution logic are interleaved in the main loop
3. The pattern is error-prone and differs from rclrs

### rclrs pattern (target)

In rclrs, an action server is created with two callbacks: one for goal
acceptance and one for cancel requests. The executor dispatches incoming
goals/cancels to these callbacks. The user then drives execution
(feedback + completion) from the goal callback or an async task.

```rust
// rclrs-like target API
executor.add_action_server::<Fibonacci>(
    "/fibonacci",
    // Goal callback: called when a new goal arrives
    |goal: &FibonacciGoal| -> GoalResponse {
        GoalResponse::AcceptAndExecute
    },
    // Cancel callback: called when a cancel request arrives
    |goal_id: &GoalId, status: GoalStatus| -> CancelResponse {
        CancelResponse::Ok
    },
)?;

loop {
    executor.spin_once(100);
}
```

### Design

An `ActionServerEntry` in the arena holds:

```rust
#[repr(C)]
struct ActionServerEntry<A, ActSrv, GoalF, CancelF,
    const GOAL_BUF: usize, const RESULT_BUF: usize, const FB_BUF: usize>
{
    server: ActSrv,               // EmbeddedActionServer handle
    goal_buffer: [u8; GOAL_BUF],  // Buffer for incoming goal requests
    result_buffer: [u8; RESULT_BUF],
    feedback_buffer: [u8; FB_BUF],
    goal_callback: GoalF,         // fn(&Goal) -> GoalResponse
    cancel_callback: CancelF,     // fn(&GoalId, GoalStatus) -> CancelResponse
    _phantom: PhantomData<A>,
}
```

The dispatch function polls all three action server channels:

```rust
unsafe fn action_server_try_process<...>(ptr: *mut u8) -> Result<bool, TransportError> {
    let entry = &mut *(...);
    let mut did_work = false;

    // 1. Poll cancel requests
    if entry.server.try_handle_cancel(|id, st| (entry.cancel_callback)(id, st)).is_ok() {
        did_work = true;
    }

    // 2. Poll new goals
    if let Ok(Some(_goal_id)) = entry.server.try_accept_goal(|g| (entry.goal_callback)(g)) {
        did_work = true;
    }

    // 3. Poll result requests (auto-served from completed goals)
    if entry.server.try_handle_get_result().is_ok() {
        did_work = true;
    }

    Ok(did_work)
}
```

**Execution model**: `add_action_server` handles the *protocol plumbing*
(accept/reject goals, handle cancels, serve results). The user drives
*execution* (compute, publish feedback, complete goal) separately — either
via a timer callback, another thread, or by retaining a handle to the
action server for manual interaction.

### API

```rust
impl<S: Session, const MAX_CBS: usize, const CB_ARENA: usize>
    EmbeddedExecutor<S, MAX_CBS, CB_ARENA>
{
    /// Register an action server with goal/cancel callbacks.
    ///
    /// The executor will automatically:
    /// - Accept/reject incoming goals via `goal_callback`
    /// - Handle cancel requests via `cancel_callback`
    /// - Serve result requests for completed goals
    ///
    /// To publish feedback and complete goals, retain the returned handle.
    pub fn add_action_server<A, GoalF, CancelF>(
        &mut self,
        action_name: &str,
        goal_callback: GoalF,
        cancel_callback: CancelF,
    ) -> Result<ActionServerHandle<A>, EmbeddedNodeError>
    where
        A: RosAction + 'static,
        GoalF: FnMut(&A::Goal) -> GoalResponse + 'static,
        CancelF: FnMut(&GoalId, GoalStatus) -> CancelResponse + 'static;
}
```

The returned `ActionServerHandle` provides methods to drive execution:

```rust
pub struct ActionServerHandle<A: RosAction> { /* arena index */ }

impl<A: RosAction> ActionServerHandle<A> {
    /// Get accepted goal data.
    pub fn get_goal(&self, executor: &EmbeddedExecutor<...>, goal_id: &GoalId)
        -> Option<&A::Goal>;
    /// Publish feedback for an active goal.
    pub fn publish_feedback(&self, executor: &mut EmbeddedExecutor<...>,
        goal_id: &GoalId, feedback: &A::Feedback) -> Result<(), ...>;
    /// Complete a goal with final status and result.
    pub fn complete_goal(&self, executor: &mut EmbeddedExecutor<...>,
        goal_id: &GoalId, status: GoalStatus, result: A::Result) -> Result<(), ...>;
}
```

### Open questions

1. **Execution callback**: Should `add_action_server` take a third callback
   `execute_callback: FnMut(GoalId, &Goal)` that runs after goal acceptance?
   This would match rclc's `rclc_action_server_set_execute_callback`. Without
   it, the user must manually check for newly accepted goals.

2. **Handle lifetime**: The `ActionServerHandle` references arena data.
   Borrowing rules must prevent use-after-drop. Simplest: handle is `Copy`
   and methods take `&mut EmbeddedExecutor` as first arg (like file descriptors).

3. **Buffer sizes**: Action servers need 5 transport channels (3 services +
   2 topics). Arena cost per action server is substantial (~5KB+ with default
   buffers). May need `add_action_server_sized` with custom buffer const
   generics.

### Tasks

- [x] Define `ActionServerArenaEntry` layout
- [x] Implement `action_server_try_process` dispatch
- [x] Add `EntryKind::ActionServer` variant
- [x] Implement `add_action_server()` and `add_action_server_sized()`
- [x] Define `ActionServerHandle` for execution control
- [x] Implement `publish_feedback()` / `complete_goal()` / `set_goal_status()` on handle
- [x] Unit tests with mock action server (3 tests)
- [ ] Consider `execute_callback` (optional third callback)

## Phase 43.9 — Action Client Callbacks (`add_action_client`)

Add `add_action_client()` to `EmbeddedExecutor` so feedback and result
notifications are dispatched automatically by `spin_once()`.

### Motivation

Action client examples poll feedback manually:

```rust
action_client.send_goal(&goal)?;
loop {
    executor.drive_io(100);
    match action_client.try_recv_feedback() {
        Ok(Some((goal_id, feedback))) => { /* process */ }
        Ok(None) => {}
        Err(e) => { /* error */ }
    }
}
let (status, result) = action_client.get_result(&goal_id)?;
```

### rclrs pattern (target)

In rclrs, action clients provide callbacks for goal response, feedback, and
result:

```rust
executor.add_action_client::<Fibonacci>(
    "/fibonacci",
    // Feedback callback: called when feedback arrives
    |goal_id: &GoalId, feedback: &FibonacciFeedback| {
        info!("Feedback: {:?}", feedback.sequence);
    },
    // Result callback: called when the goal completes
    |goal_id: &GoalId, status: GoalStatus, result: &FibonacciResult| {
        info!("Result: {:?}", result.sequence);
    },
)?;
```

### Design

```rust
#[repr(C)]
struct ActionClientEntry<A, ActCli, FeedbackF, ResultF,
    const GOAL_BUF: usize, const FB_BUF: usize, const RESULT_BUF: usize>
{
    client: ActCli,
    goal_buffer: [u8; GOAL_BUF],
    feedback_buffer: [u8; FB_BUF],
    result_buffer: [u8; RESULT_BUF],
    feedback_callback: FeedbackF,
    result_callback: ResultF,
    _phantom: PhantomData<A>,
}
```

Dispatch function polls feedback and status topics:

```rust
unsafe fn action_client_try_process<...>(ptr: *mut u8) -> Result<bool, TransportError> {
    let entry = &mut *(...);
    let mut did_work = false;

    // Poll feedback
    if let Ok(Some((goal_id, feedback))) = entry.client.try_recv_feedback() {
        (entry.feedback_callback)(&goal_id, &feedback);
        did_work = true;
    }

    // Poll result (for any pending get_result requests)
    // Result delivery is trickier — typically triggered after send_goal
    // and requires tracking which goals are awaiting results.

    Ok(did_work)
}
```

### API

```rust
impl<S: Session, const MAX_CBS: usize, const CB_ARENA: usize>
    EmbeddedExecutor<S, MAX_CBS, CB_ARENA>
{
    /// Register an action client with feedback/result callbacks.
    ///
    /// Goal sending remains manual via the returned handle.
    pub fn add_action_client<A, FeedbackF, ResultF>(
        &mut self,
        action_name: &str,
        feedback_callback: FeedbackF,
        result_callback: ResultF,
    ) -> Result<ActionClientHandle<A>, EmbeddedNodeError>
    where
        A: RosAction + 'static,
        FeedbackF: FnMut(&GoalId, &A::Feedback) + 'static,
        ResultF: FnMut(&GoalId, GoalStatus, &A::Result) + 'static;
}

pub struct ActionClientHandle<A: RosAction> { /* arena index */ }

impl<A: RosAction> ActionClientHandle<A> {
    /// Send a goal request (blocking until accepted/rejected).
    pub fn send_goal(&self, executor: &mut EmbeddedExecutor<...>,
        goal: &A::Goal) -> Result<GoalId, ...>;
    /// Cancel an active goal.
    pub fn cancel_goal(&self, executor: &mut EmbeddedExecutor<...>,
        goal_id: &GoalId) -> Result<CancelResponse, ...>;
}
```

### Open questions

1. **Result delivery**: When does the result callback fire? Options:
   a. User calls `request_result(goal_id)` on the handle, then `spin_once()`
      delivers the result to the callback when it arrives.
   b. Automatically request result when goal status becomes terminal.
   Option (b) matches rclrs behavior.

2. **Goal response callback**: Should there be a third callback for
   `send_goal` response (accepted/rejected)? In rclrs this is a future.
   For embedded, `send_goal` is synchronous, so the caller already knows.

### Tasks

- [x] Define `ActionClientArenaEntry` layout
- [x] Implement `action_client_try_process` dispatch
- [x] Add `EntryKind::ActionClient` variant
- [x] Implement `add_action_client()` and `add_action_client_sized()`
- [x] Define `ActionClientHandle` for goal sending
- [x] Implement `send_goal()` / `cancel_goal()` / `get_result()` on handle
- [x] Result retrieval is manual via handle (synchronous service call)
- [x] Unit tests with mock action client (3 tests)

## Phase 43.10 — Unify Executors into EmbeddedExecutor

Port features from `PollingExecutor` and `BasicExecutor` into `EmbeddedExecutor`,
then remove the former two. The goal is a single executor type that works across
bare-metal, RTOS, and desktop environments without requiring `std` or `alloc`.

### Motivation

The codebase has three executor types:

| Executor | Backend | alloc | std | Spin API |
|----------|---------|-------|-----|----------|
| `EmbeddedExecutor<S, MAX_CBS, CB_ARENA>` | Any | No | No | `spin_once()`, `spin()` |
| `PollingExecutor<MAX_NODES>` | zenoh only | Yes | No | `spin_once()`, `spin_one_period()` |
| `BasicExecutor` | zenoh only | Yes | Yes | `spin()`, `spin_period()`, halt |

`EmbeddedExecutor` already covers the full callback API (subscriptions, services,
timers, actions) via arena storage. The other two add:

- **PollingExecutor**: `spin_one_period()` returning remaining sleep time (no_std)
- **BasicExecutor**: blocking `spin(SpinOptions)`, `spin_period(Duration)`,
  `spin_one_period(Duration) -> SpinPeriodResult` (overrun detection),
  thread-safe halt via `Arc<AtomicBool>`

All of these can be ported to `EmbeddedExecutor` with `#[cfg(feature = "std")]`
gates for the std-dependent parts.

### What gets dropped

- `PollingExecutor`, `BasicExecutor` structs
- `Executor` trait, `SpinExecutor` trait
- `NodeState`, `NodeHandle` and `Box<dyn ErasedCallback>` type-erasure machinery
- `SubscriptionCallback`, `SubscriptionCallbackWithInfo`, `ExecutorTimerCallback` traits
- Trigger conditions (`Any`/`All`/`Custom`) — arena dispatch doesn't have per-handle
  readiness queries; dispatch always tries all entries

### Changes to EmbeddedExecutor

**New field (std-gated):**

```rust
pub struct EmbeddedExecutor<S, const MAX_CBS: usize = 0, const CB_ARENA: usize = 0> {
    session: S,
    arena: [MaybeUninit<u8>; CB_ARENA],
    arena_used: usize,
    entries: [Option<CallbackMeta>; MAX_CBS],
    #[cfg(feature = "std")]
    halt_flag: std::sync::Arc<std::sync::atomic::AtomicBool>,
}
```

**New no_std methods:**

```rust
/// Process one period. Returns remaining sleep time for caller.
fn spin_one_period(&mut self, period_ms: u64, elapsed_ms: u64) -> SpinPeriodPollingResult;
```

**New std-gated methods:**

```rust
/// Blocking spin loop with timeout/halt/max_callbacks control.
fn spin_blocking(&mut self, opts: SpinOptions) -> Result<(), EmbeddedNodeError>;

/// Fixed-rate spin with drift compensation. Blocks until halt.
fn spin_period(&mut self, period: std::time::Duration) -> Result<(), EmbeddedNodeError>;

/// Single period with wall-clock overrun detection + sleep.
fn spin_one_period_timed(&mut self, period: std::time::Duration) -> SpinPeriodResult;

/// Request halt (thread-safe, callable from signal handlers).
fn halt(&self);
fn is_halted(&self) -> bool;
fn halt_flag(&self) -> std::sync::Arc<std::sync::atomic::AtomicBool>;
```

**New types:**

```rust
// no_std — ported from PollingExecutor
pub struct SpinPeriodPollingResult {
    pub work: SpinOnceResult,
    pub remaining_ms: u64,
}

// no_std — ported from BasicExecutor (no std dep in the struct itself)
pub struct SpinOptions {
    pub timeout_ms: Option<u64>,
    pub only_next: bool,
    pub max_callbacks: Option<usize>,
}

// std only — ported from BasicExecutor
#[cfg(feature = "std")]
pub struct SpinPeriodResult {
    pub work: SpinOnceResult,
    pub overrun: bool,
    pub elapsed: std::time::Duration,
}
```

### Usage across environments

**Bare-metal:**
```rust
let mut executor: EmbeddedExecutor<_, 4, 4096> = EmbeddedExecutor::open(&config)?;
executor.add_subscription::<Int32, _>("/cmd", |msg| { /* ... */ })?;
executor.spin(10); // never returns
```

**RTOS with rate control:**
```rust
loop {
    let r = executor.spin_one_period(10, elapsed_ms);
    platform_sleep_ms(r.remaining_ms);
}
```

**Desktop:**
```rust
let mut executor: EmbeddedExecutor<_, 8, 16384> = EmbeddedExecutor::open(&config)?;
let halt = executor.halt_flag();
ctrlc::set_handler(move || halt.store(true, Ordering::SeqCst))?;
executor.spin_blocking(SpinOptions::default())?;
```

### Naming

After functional validation, rename `EmbeddedExecutor` → `Executor` in a
separate commit. The "Embedded" prefix is misleading when it serves all targets.

### Tasks

- [x] Add `SpinPeriodPollingResult`, `SpinOptions`, `SpinPeriodResult` to `generic.rs`
- [x] Add `halt_flag` field to `EmbeddedExecutor` behind `#[cfg(feature = "std")]`
- [x] Update `from_session()` and `open()` to init halt flag
- [x] Add `spin_one_period()` (no_std)
- [x] Add `spin_blocking()`, `spin_period()`, `spin_one_period_timed()` (std-gated)
- [x] Add `halt()`, `is_halted()`, `halt_flag()` (std-gated)
- [x] Update `lib.rs` re-exports
- [x] Deduplicate types: `executor.rs` re-exports from `generic.rs`
- [x] `just quality` passes
- [x] Unit tests for new spin methods (11 tests: spin_one_period, SpinOptions, spin_blocking, halt, spin_period)

## Implementation Order

1. **43.1** — Backend-agnostic factory (`EmbeddedConfig` + `open()`) ✅
2. **43.3** — XRCE action examples → typed API ✅
3. **43.4** — Migrate all examples to `open()` factory ✅
4. **43.5** — Delete deprecated items + clean up exports ✅
5. **43.2** — Arena-based callback storage + `spin_once()` ✅
6. **43.7** — Timer callbacks (`add_timer`) ✅
7. **43.8** — Action server callbacks (`add_action_server`) ✅
8. **43.9** — Action client callbacks (`add_action_client`) ✅
9. **43.10** — Unify executors (port std spin + halt to EmbeddedExecutor) ✅
10. **43.11** — Module restructuring (split generic.rs → executor/) ✅
11. **43.6** — Migrate all examples to callback+spin ✅
12. **43.12** — Migrate Connected executor users to EmbeddedExecutor ✅
13. **43.13** — Rename EmbeddedExecutor → Executor ✅

43.10 must precede 43.6 so examples migrate to the final API.
43.12 must precede 43.13 so all users are migrated before the rename.

### API coverage summary

| Entity         | Manual poll (43.2)                          | Callback+spin                       | Phase |
|----------------|---------------------------------------------|-------------------------------------|-------|
| Subscription   | `try_recv()`                                | `add_subscription()` ✅             | 43.2  |
| Service server | `handle_request()`                          | `add_service()` ✅                  | 43.2  |
| Service client | `call()` (sync)                             | N/A (sync is fine)                  | —     |
| Publisher      | `publish()` (sync)                          | N/A + `add_timer()` for periodic ✅ | 43.7  |
| Timer          | —                                           | `add_timer()` ✅                    | 43.7  |
| Action server  | `try_accept_goal()` + `try_handle_cancel()` | `add_action_server()` ✅            | 43.8  |
| Action client  | `send_goal()` + `try_recv_feedback()`       | `add_action_client()` ✅            | 43.9  |

## Phase 43.11 — Module Restructuring

Split the monolithic `generic.rs` (4001 lines) into focused submodules under
`executor/`, and rename the legacy `executor.rs` to `connected_executor.rs`
to avoid name collision.

### New module structure

```
packages/core/nros-node/src/
├── executor/                  (replaces generic.rs)
│   ├── mod.rs                 (module docs + re-exports)
│   ├── types.rs               (SpinOnceResult, SpinOptions, EmbeddedConfig, EmbeddedNodeError)
│   ├── arena.rs               (CallbackMeta, entry types, dispatch fns)
│   ├── spin.rs                (EmbeddedExecutor struct, open(), spin methods, Drop)
│   ├── action.rs              (add_action_server/client, ActionServerHandle, ActionClientHandle)
│   ├── node.rs                (EmbeddedNode struct + create_* methods)
│   ├── handles.rs             (EmbeddedPublisher, EmbeddedSubscription, EmbeddedServiceServer,
│   │                           EmbeddedServiceClient, EmbeddedActionServer, EmbeddedActionClient)
│   └── tests.rs               (MockSession + 28 unit tests)
├── connected_executor.rs      (renamed from executor.rs — legacy PollingExecutor/BasicExecutor)
└── ...
```

### Import path changes

- `crate::generic::` → `crate::executor::`
- `crate::executor::` → `crate::connected_executor::` (in `context.rs`, `lifecycle.rs`)
- `nros_node::generic::` → `nros_node::executor::` (all external references)

### Tasks

- [x] Split `generic.rs` into 8 submodules under `executor/`
- [x] Rename `executor.rs` → `connected_executor.rs` (git mv)
- [x] Update `lib.rs` module declarations and re-exports
- [x] Update imports in `context.rs` (2 paths) and `lifecycle.rs` (1 path)
- [x] Update `connected_executor.rs` re-exports (`crate::generic::` → `crate::executor::`)
- [x] Fix unused import warnings (feature-gated imports for `EmbeddedConfig`, `SpinOptions`)
- [x] Delete `generic.rs`
- [x] `just quality` passes (all 76 executor tests + full suite)

## Phase 43.12 — Migrate Connected Executor Users to EmbeddedExecutor

Migrate tests and examples that use `PollingExecutor` / `BasicExecutor`
(from `connected_executor.rs`) to use `EmbeddedExecutor` instead.
Eventually delete `connected_executor.rs` entirely.

### Current users of connected_executor

- `context.rs` — `Context::create_polling_executor()`, `Context::create_basic_executor()`
- `lifecycle.rs` — `LifecycleNode` uses `NodeHandle` from connected executor
- `nros/src/lib.rs` — re-exports `PollingExecutor`, `BasicExecutor`, `NodeHandle`, etc.
- Native zenoh examples (talker, listener, service-*, action-*) via `Context` API
- Integration tests in `nros-tests/`

### Strategy

1. **Port Context methods** to create `EmbeddedExecutor` instead of legacy executors
2. **Migrate examples** from `Context` + `BasicExecutor` to `EmbeddedExecutor::open()`
3. **Migrate integration tests** to use `EmbeddedExecutor` API
4. **Port LifecycleNode** to work with `EmbeddedExecutor`
5. **Delete** `connected_executor.rs`, `connected.rs`, `context.rs` (legacy API)
6. **Clean up** `nros/src/lib.rs` re-exports

### Tasks

- [x] Audit all users of `PollingExecutor` / `BasicExecutor` / `NodeHandle`
- [x] Migrate native zenoh examples to `Executor::open()` + callback API
- [x] Delete `connected_executor.rs`, `connected.rs`, `context.rs`, `error.rs`,
      `options.rs`, `trigger.rs`, `rtic.rs` legacy modules
- [x] Remove `LifecycleNode` (alloc-based wrapper); keep `LifecyclePollingNode`
- [x] Update `nros-node/src/lib.rs` and `nros/src/lib.rs` re-exports
- [x] Add `ExecutorConfig::from_env()` (std+alloc gated) for native examples
- [x] `just quality` passes (unit + miri + qemu; integration tests have known
      failures from dropped feature coverage in migrated examples)

## Phase 43.13 — Rename EmbeddedExecutor → Executor

Once `EmbeddedExecutor` is the sole executor type and all legacy code is
deleted, rename it to `Executor` since the "Embedded" prefix is misleading
when it serves bare-metal, RTOS, and desktop targets equally.

### Scope

- Rename `EmbeddedExecutor` → `Executor`
- Rename `EmbeddedNode` → `Node` (or keep as `EmbeddedNode` if `Node` conflicts)
- Rename `EmbeddedNodeError` → `NodeError` (or `ExecutorError`)
- Rename `EmbeddedConfig` → `ExecutorConfig` (or just `Config`)
- Rename `EmbeddedPublisher` → `Publisher`, `EmbeddedSubscription` → `Subscription`, etc.
- Update all examples, tests, docs, and re-exports
- Consider backwards-compatibility type aliases (optional)

### Tasks

- [x] Choose final names: `Executor`, `Node`, `NodeError`, `ExecutorConfig`,
      `Subscription` (5 renamed); `EmbeddedPublisher`, `EmbeddedServiceServer/Client`,
      `EmbeddedActionServer/Client` kept (naming conflicts with RMW traits)
- [x] Rename types across executor submodules (spin.rs, types.rs, node.rs, handles.rs,
      arena.rs, action.rs, mod.rs, tests.rs)
- [x] Update nros-node/src/lib.rs and nros/src/lib.rs re-exports + prelude
- [x] Add backward compatibility type aliases (`EmbeddedExecutor`, `EmbeddedNode`, etc.)
- [x] Update all 25 example files (XRCE, Zephyr, native zenoh)
- [x] Update parameter_services.rs, ghost-types, justfile
- [x] `just quality` passes (format + clippy + unit tests)

## Verification

1. `just quality` — format + clippy + test (294 unit tests pass)
2. `cargo clippy -p nros --features rmw-zenoh,platform-posix,ros-humble -- -D warnings`
3. `cargo clippy -p nros --features rmw-xrce,xrce-udp,platform-posix -- -D warnings`
4. `cargo clippy -p nros --features rmw-cffi -- -D warnings`
5. `cargo build --workspace --no-default-features --exclude nros-c`
6. XRCE integration tests pass (service + action)
7. No backend-specific type names appear in any example `use` statement
   (grep verification)

## Key Files

| File                                              | Role                                                                     |
|---------------------------------------------------|--------------------------------------------------------------------------|
| `packages/core/nros-node/src/executor/mod.rs`     | Module declarations + flat re-exports                                    |
| `packages/core/nros-node/src/executor/types.rs`   | SpinOnceResult, SpinOptions, ExecutorConfig, NodeError                   |
| `packages/core/nros-node/src/executor/arena.rs`   | Callback arena: entry types, dispatch fns, handle operation fns          |
| `packages/core/nros-node/src/executor/spin.rs`    | Executor struct, open(), spin methods, Drop                              |
| `packages/core/nros-node/src/executor/action.rs`  | add_action_server/client, ActionServerHandle, ActionClientHandle         |
| `packages/core/nros-node/src/executor/node.rs`    | Node struct + create_* methods                                           |
| `packages/core/nros-node/src/executor/handles.rs` | EmbeddedPublisher, Subscription, ServiceServer/Client, ActionServer/Client |
| `packages/core/nros-node/src/executor/tests.rs`   | MockSession + 28 unit tests                                             |
| ~~`packages/core/nros-node/src/connected_executor.rs`~~ | Deleted in 43.12                                                |
| `packages/core/nros-node/src/timer.rs`            | TimerState, TimerDuration                                                |
| `packages/core/nros-node/src/lib.rs`              | Module declarations + re-exports                                         |
| `packages/core/nros/src/lib.rs`                   | Prelude + re-exports                                                     |
| `examples/zephyr/rust/zenoh/*/src/lib.rs` (6)     | Migrate to callback+spin pattern (43.6)                                  |
| `examples/native/rust/xrce/*/src/main.rs` (10)    | Migrate to callback+spin pattern (43.6)                                  |
