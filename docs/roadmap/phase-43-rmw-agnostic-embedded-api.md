# Phase 43 — RMW-Agnostic Embedded API

## Status: Not Started

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

The factory lives in `nros-node/src/generic.rs` but requires
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

- [ ] Define `EmbeddedConfig` struct with builder methods
- [ ] Add optional backend deps to `nros-node/Cargo.toml` (or implement
      factory in `nros/src/lib.rs` only)
- [ ] Implement `EmbeddedExecutor::open()` for zenoh
- [ ] Implement `EmbeddedExecutor::open()` for XRCE (with transport auto-init)
- [ ] Implement `EmbeddedExecutor::open()` for cffi
- [ ] Add `EmbeddedConfig` and factory to prelude
- [ ] Update `internals::open_session` to delegate to the new factory (or
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

| Component | Size |
|-----------|------|
| Subscriber handle (`XrceSubscriber`) | ~32 bytes |
| Subscriber handle (`ShimSubscriber`) | ~16 bytes |
| Receive buffer (`RX_BUF=1024`) | 1024 bytes |
| Callback (`fn(&Int32)`) | 0 bytes (ZST) |
| PhantomData | 0 bytes |
| **Total per entry (XRCE)** | **~1056 bytes** |
| **Total per entry (zenoh)** | **~1040 bytes** |

So `CB_ARENA=4096` fits ~3-4 subscriptions with 1KB receive buffers.
`CB_ARENA=8192` fits ~7-8. Users needing large receive buffers
(e.g., `RX_BUF=4096`) should size the arena accordingly.

### Tasks

- [ ] Add const generics `MAX_CBS` and `CB_ARENA` to `EmbeddedExecutor`
      (with defaults `0, 0`)
- [ ] Define `CallbackMeta` struct
- [ ] Define `ConcreteEntry` layout
- [ ] Implement arena bump allocation with alignment
- [ ] Implement monomorphized `try_process_impl` function
- [ ] Implement monomorphized `drop_impl` function
- [ ] Add `add_subscription()` and `add_subscription_sized()` methods
- [ ] Add `add_service()` method
- [ ] Add `spin_once()` method
- [ ] Add `spin()` blocking loop (behind `std`)
- [ ] Add `Drop` impl for `EmbeddedExecutor` to call `drop_fn` on entries
- [ ] Re-use `SpinOnceResult` from `executor.rs`

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

- [ ] Rewrite `examples/native/rust/xrce/action-server/src/main.rs` to use
      `EmbeddedActionServer`
- [ ] Rewrite `examples/native/rust/xrce/action-client/src/main.rs` to use
      `EmbeddedActionClient`
- [ ] Remove `XrceSession` and raw CDR imports from action examples
- [ ] Verify action integration tests still pass

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

- [ ] Migrate 6 Zephyr examples to `EmbeddedExecutor::open()`
- [ ] Migrate 8 XRCE pub/sub/service examples to `EmbeddedExecutor::open()`
- [ ] Verify: no backend-specific types in any example's `use` statements
- [ ] Optional: convert listener examples to callback+spin pattern

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

- [ ] Delete all 14 deprecated methods listed above
- [ ] Update internal callers (`context.rs:418`, `executor.rs:1308`,
      `executor.rs:1522` use `#[allow(deprecated)]` for `ConnectedNode::new`
      — rewrite to use non-deprecated paths)
- [ ] Move XRCE raw re-exports to `internals` module
- [ ] Delete `xrce_transport` module (move to `internals`)
- [ ] Remove `alloc` gate from `internals::open_session()`
- [ ] Update prelude to include `EmbeddedConfig`
- [ ] Remove all `#[allow(deprecated)]` from examples
- [ ] Verify no backend-specific types in `nros::` root
- [ ] `just quality` passes

## Implementation Order

1. **43.1** — Backend-agnostic factory (`EmbeddedConfig` + `open()`)
2. **43.3** — XRCE action examples → typed API (independent of 43.2)
3. **43.4** — Migrate all examples to `open()` factory
4. **43.2** — Arena-based callback storage + `spin_once()`
5. **43.4 (stretch)** — Convert listener examples to callback+spin
6. **43.5** — Delete deprecated items + clean up exports

43.1 and 43.3 can proceed in parallel. 43.2 is independent of the
example migration and can be done before or after. 43.5 should be last
since it removes APIs that internal code currently uses.

## Verification

1. `just quality` — format + clippy + test (all 422+ tests pass)
2. `cargo clippy -p nros --features rmw-zenoh,platform-posix,ros-humble -- -D warnings`
3. `cargo clippy -p nros --features rmw-xrce,xrce-udp,platform-posix -- -D warnings`
4. `cargo clippy -p nros --features rmw-cffi -- -D warnings`
5. `cargo build --workspace --no-default-features --exclude nros-c`
6. XRCE integration tests pass (service + action)
7. No backend-specific type names appear in any example `use` statement
   (grep verification)

## Key Files

| File                                           | Action                                                                  |
|------------------------------------------------|-------------------------------------------------------------------------|
| `packages/core/nros-node/src/generic.rs`       | Add `EmbeddedConfig`, `open()` factory, callback arena, `spin_once()`   |
| `packages/core/nros/src/lib.rs`                | Move XRCE re-exports to internals, update prelude, remove alloc gate    |
| `packages/core/nros-node/Cargo.toml`           | Optional deps on backend crates (if factory lives here)                 |
| `packages/core/nros-node/src/connected.rs`     | Delete 8 deprecated methods                                            |
| `packages/core/nros-node/src/node.rs`          | Delete 4 deprecated methods                                            |
| `packages/core/nros-node/src/context.rs`       | Delete `create_executor()`, update internal callers                     |
| `packages/core/nros-node/src/executor.rs`      | Update internal `#[allow(deprecated)]` callers                          |
| `examples/zephyr/rust/zenoh/*/src/lib.rs` (6)  | Migrate to `EmbeddedExecutor::open()`                                   |
| `examples/native/rust/xrce/*/src/main.rs` (10) | Migrate to `EmbeddedExecutor::open()` + typed actions                   |
