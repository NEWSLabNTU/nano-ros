# Phase 49 — nros-c Thin Wrapper Migration

## Status: Not Started

## Background

nros-c (`packages/core/nros-c/`) is the C FFI layer for nano-ros. Per the
CLAUDE.md principle:

> nros-c must be a thin FFI wrapper over the Rust nros-node API — it should
> expose `#[unsafe(no_mangle)] extern "C"` functions that delegate to nros-node
> types, not reimplement logic.

In practice, nros-c contains ~10,069 lines of Rust across 19 source files.
Several modules — executor, timer, guard condition, and action — implement
their own state machines, dispatch loops, and data structures rather than
delegating to nros-node. The executor alone is 1,788 lines with its own
handle management, trigger conditions, LET semantics, and spin logic. This
duplication means:

- **Bug fixes must be applied twice** (once in nros-node, once in nros-c)
- **New features** (trigger conditions from Phase 47, future scheduling
  policies) must be re-implemented in C-style Rust
- **Formal verification** (Kani/Verus) covers one implementation but not
  the other
- **Testing burden** doubles for executor-level behavior

### Current nros-c Architecture

```
nros-c/src/executor.rs  → wraps nros-rmw directly (self-implemented dispatch)
nros-c/src/timer.rs     → self-implemented period tracking
nros-c/src/guard_condition.rs → self-implemented atomic flag
nros-c/src/action.rs    → self-implemented goal state machine

nros-c  →  nros-rmw (transport traits)
nros-c  ✗  does NOT delegate to nros-node executor
```

Phase 47 (prerequisite) adds trigger conditions and `InvocationMode` to
nros-node. Phase 49 adds the remaining missing capabilities to nros-node
(raw-bytes callbacks, guard conditions, LET semantics, session-borrowing
executor) and rewrites nros-c to delegate.

---

## Goals

1. **Eliminate duplicated executor logic** — nros-c delegates to nros-node's
   `Executor` for spin, trigger evaluation, handle management, and dispatch
2. **Add raw-bytes callback support to nros-node** — C callbacks receive
   `(*const u8, usize)`, not typed `&M`; nros-node must support this natively
3. **Add guard conditions to nros-node** — atomic signal type usable from
   any thread or ISR
4. **Add LET semantics to nros-node** — logical execution time: sample all
   subscriptions at spin start, process from snapshot
5. **Preserve C API compatibility** — all existing C headers, function
   signatures, and struct layouts remain unchanged
6. **Reduce nros-c line count** — target ~60% reduction in migrated modules

## Non-Goals

- Migrating C-specific modules (CDR marshaling, `#[repr(C)]` structs,
  publisher/subscription init) — these are inherently FFI-boundary code
- Changing the C header (`nano_ros/executor.h`) public interface
- Adding new C API features — this phase is purely structural migration
- Removing the `#![allow(unsafe_op_in_unsafe_fn)]` attribute from nros-c

---

## Current State Inventory

| Module | Lines | Category | Notes |
|--------|------:|----------|-------|
| `executor.rs` | 1,788 | Self-implemented | Dispatch, spin, triggers, LET, handle mgmt |
| `action.rs` | 1,086 | Self-implemented | Goal state machine, UUID tracking |
| `cdr.rs` | 1,174 | C-specific | Raw CDR serialization for C types |
| `parameter.rs` | 1,222 | C-specific | Parameter server C bindings |
| `service.rs` | 838 | C-specific | Service server/client init + raw dispatch |
| `lifecycle.rs` | 728 | C-specific | Lifecycle state machine C bindings |
| `publisher.rs` | 549 | C-specific | Publisher init + raw publish |
| `subscription.rs` | 465 | C-specific | Subscription init + raw recv |
| `guard_condition.rs` | 450 | Self-implemented | Atomic flag + callback |
| `node.rs` | 349 | C-specific | Metadata container |
| `timer.rs` | 348 | Self-implemented | Period tracking + callback |
| `clock.rs` | 315 | C-specific | Clock C bindings |
| `support.rs` | 283 | C-specific | Session/support init |
| `platform.rs` | 183 | C-specific | Platform abstraction C bindings |
| `qos.rs` | 122 | C-specific | QoS profile C bindings |
| `lib.rs` | 85 | C-specific | Module declarations |
| `error.rs` | 50 | C-specific | Error code definitions |
| `constants.rs` | 28 | C-specific | Build-time constants |
| `config.rs` | 6 | C-specific | Config re-exports |
| **Total** | **10,069** | | |

**Migration targets** (self-implemented): executor.rs, timer.rs,
guard_condition.rs, action.rs = **3,672 lines** (36% of crate).

**Expected post-migration**: ~1,100 lines (thin delegation wrappers).

---

## Design Decisions

### Ownership Model: Session-Borrowing Executor

In the C API, the support object (`nano_ros_support_t`) owns the session and
is created before the executor. The executor borrows the session for its
lifetime. nros-node's `Executor<S>` currently owns the session (created via
`Executor::open()`).

To support the C API pattern, add an `unsafe` constructor that accepts a raw
pointer to an externally-owned session:

```rust
pub unsafe fn from_session_ptr(session_ptr: *mut S) -> Self
```

The executor stores `*mut S` and dereferences it in `drive_io()` /
`spin_once()`. The caller guarantees the session outlives the executor.

### What Migrates vs What Stays

**Migrates to nros-node (then nros-c wraps):**
- Executor spin loop, trigger evaluation, dispatch
- Timer period tracking and readiness detection
- Guard condition atomic flag and callback
- Action goal state machine and concurrent goal tracking

**Stays in nros-c (inherently C-specific):**
- Publisher/subscription/service init (creates RMW types directly — C has no
  generics for typed wrappers)
- CDR marshaling (`cdr.rs`)
- Parameter server bindings (`parameter.rs`)
- Lifecycle bindings (`lifecycle.rs`)
- Node metadata container (`node.rs`)
- All `#[repr(C)]` struct definitions
- QoS, clock, platform, error, constants modules

### Raw-Bytes Callback Approach

C callbacks cannot receive typed `&M` references — they work with raw CDR
bytes. Rather than deserializing in nros-node and re-serializing for C (which
would be pointless overhead), add raw-bytes entry variants to the arena:

```rust
pub type RawSubscriptionCallback =
    unsafe extern "C" fn(data: *const u8, len: usize, context: *mut c_void);

pub type RawServiceCallback =
    unsafe extern "C" fn(
        req: *const u8, req_len: usize,
        resp: *mut u8, resp_cap: usize, resp_len: *mut usize,
        context: *mut c_void,
    ) -> bool;
```

These bypass deserialization entirely — `try_recv_raw()` returns the CDR
buffer, which is passed directly to the C callback. The raw variants live
alongside the typed variants in the arena and participate identically in
trigger evaluation, invocation mode checks, and LET sampling.

---

## Sub-phases

### 49.1 — Raw-bytes Callbacks in nros-node

Add raw-bytes entry types to the executor arena, enabling callbacks that
receive CDR bytes without deserialization. This is the foundation for nros-c
delegation.

**Arena entry types:**

- `SubRawEntry<Sub, const RX_BUF: usize>` — subscriber handle + raw fn ptr +
  context pointer + receive buffer
- `SrvRawEntry<Srv, const REQ_BUF: usize, const REPLY_BUF: usize>` — service
  server handle + raw fn ptr + context pointer + request/reply buffers

**Dispatch functions:**

- `sub_raw_try_process()` — calls `try_recv_raw()` on subscriber, invokes
  raw callback with `(data_ptr, data_len, context)`
- `srv_raw_try_process()` — calls `try_recv_request()` on service server,
  invokes raw callback with request bytes, collects reply bytes, calls
  `send_reply()` with raw reply

**Registration methods:**

- `Executor::add_subscription_raw()` → `HandleId`
- `Executor::add_service_raw()` → `HandleId`

**Callback type definitions:**

```rust
pub type RawSubscriptionCallback =
    unsafe extern "C" fn(data: *const u8, len: usize, context: *mut c_void);

pub type RawServiceCallback =
    unsafe extern "C" fn(
        req: *const u8, req_len: usize,
        resp: *mut u8, resp_cap: usize, resp_len: *mut usize,
        context: *mut c_void,
    ) -> bool;
```

**Has-data functions:**

- `sub_raw_has_data()` — delegates to `subscriber.has_data()`
- `srv_raw_has_data()` — delegates to `server.has_request()`

**Tasks:**

- [ ] Define `RawSubscriptionCallback` and `RawServiceCallback` type aliases
  in `executor/types.rs`
- [ ] Add `SubRawEntry<Sub, RX_BUF>` struct to `executor/arena.rs`
- [ ] Add `SrvRawEntry<Srv, REQ_BUF, REPLY_BUF>` struct to `executor/arena.rs`
- [ ] Implement `sub_raw_try_process()` in `executor/arena.rs`
- [ ] Implement `srv_raw_try_process()` in `executor/arena.rs`
- [ ] Implement `sub_raw_has_data()` and `srv_raw_has_data()`
- [ ] Add `Executor::add_subscription_raw()` to `executor/spin.rs`
- [ ] Add `Executor::add_service_raw()` to `executor/spin.rs`
- [ ] Add `EntryKind::SubscriptionRaw` and `EntryKind::ServiceRaw` variants
  (or reuse existing kinds)
- [ ] Unit tests for raw subscription dispatch
- [ ] Unit tests for raw service dispatch

**Files:** `nros-node/src/executor/{arena.rs, spin.rs, types.rs}`

---

### 49.2 — Guard Conditions in nros-node

nros-node currently has no guard condition type. Guard conditions are manual
triggers — an atomic flag that can be set from any thread or ISR to wake the
executor. They are used for shutdown signaling, inter-thread notifications,
and custom event injection.

**New type:**

```rust
pub struct GuardCondition {
    triggered: AtomicBool,
    callback: Option<(unsafe extern "C" fn(*mut c_void), *mut c_void)>,
}
```

**Arena integration:**

- `GuardConditionEntry` — guard condition + callback fn + context
- `guard_try_process()` — check flag, clear, invoke callback if set
- `guard_has_data()` — check triggered flag

**Registration:**

- `Executor::add_guard_condition()` → `HandleId`

**Tasks:**

- [ ] Create `nros-node/src/guard_condition.rs` with `GuardCondition` struct
- [ ] Implement `trigger()`, `is_triggered()`, `clear()` methods
- [ ] Implement optional callback storage and invocation
- [ ] Ensure `Send + Sync` safety (atomic flag is inherently thread-safe)
- [ ] Add `GuardConditionEntry` to `executor/arena.rs`
- [ ] Implement `guard_try_process()` dispatch function
- [ ] Implement `guard_has_data()` readiness function
- [ ] Add `Executor::add_guard_condition()` to `executor/spin.rs`
- [ ] Add `EntryKind::GuardCondition` variant to `executor/types.rs`
- [ ] Integrate into spin_once readiness scan (Phase 47's three-phase flow)
- [ ] Unit tests for guard condition trigger/clear/callback
- [ ] Unit tests for executor integration (spin_once processes guard conditions)

**Files:** new `nros-node/src/guard_condition.rs`,
`nros-node/src/executor/{arena.rs, spin.rs, types.rs, mod.rs}`

---

### 49.3 — LET Semantics in nros-node

Logical Execution Time (LET) semantics: sample all subscription data at the
start of each spin cycle, then process callbacks from the snapshot. This
prevents data races where a callback sees newer data than earlier callbacks
in the same cycle. nros-c implements this in `executor.rs:844-873`.

**New types:**

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ExecutorSemantics {
    /// Standard interleaved execution (default). Each callback sees the
    /// latest data at the time it runs.
    #[default]
    RclcppExecutor,

    /// Logical Execution Time. All subscriptions are sampled at spin start;
    /// callbacks process from the snapshot.
    LogicalExecutionTime,
}
```

**Mechanism:**

- Add `semantics: ExecutorSemantics` field to `Executor`
- Add `Executor::set_semantics()` method
- Modify spin_once: if LET, iterate all subscription entries and call
  `try_recv_raw()` / `try_recv()` into per-entry LET buffer before the
  dispatch phase. During dispatch, callbacks read from the LET buffer
  instead of calling recv again.
- Services always use immediate mode (request-reply is inherently sequential)
- LET buffer field added to `SubEntry` / `SubRawEntry` (compile-time sized
  via const generic `RX_BUF`)

**Tasks:**

- [ ] Define `ExecutorSemantics` enum in `executor/types.rs`
- [ ] Add `semantics` field to `Executor` struct
- [ ] Add `Executor::set_semantics()` public method
- [ ] Add LET buffer field to `SubEntry` and `SubRawEntry`
- [ ] Add `try_sample()` function that copies latest data into LET buffer
- [ ] Modify `sub_try_process()` to read from LET buffer when in LET mode
- [ ] Modify `sub_raw_try_process()` similarly
- [ ] Add pre-sample phase to `spin_once()` between readiness scan and
  dispatch (only when `semantics == LogicalExecutionTime`)
- [ ] Unit tests for LET semantics (data sampled once, consistent snapshot)
- [ ] Unit tests verifying default RclcppExecutor behavior is unchanged

**Files:** `nros-node/src/executor/{types.rs, spin.rs, arena.rs}`

---

### 49.4 — Session-Borrowing Executor

nros-node's `Executor<S>` owns the session. The C API requires the support
object to own the session, with the executor borrowing it. Add an unsafe
constructor that accepts a raw pointer to an externally-owned session.

**API:**

```rust
impl<S: Session, const MAX_CBS: usize, const CB_ARENA: usize>
    Executor<S, MAX_CBS, CB_ARENA>
{
    /// Create an executor that borrows a session via raw pointer.
    ///
    /// # Safety
    /// - `session_ptr` must point to a valid, initialized `S`
    /// - The session must outlive the executor
    /// - No other code may move or drop the session while the executor exists
    pub unsafe fn from_session_ptr(session_ptr: *mut S) -> Self { ... }
}
```

**Implementation options:**

1. Store `*mut S` in a newtype that derefs to `&mut S` — executor code
   unchanged
2. Store an enum `Owned(S) | Borrowed(*mut S)` — dispatch on variant

Option 1 is simpler; option 2 avoids UB if the raw pointer is ever null.
Recommend option 2 with a debug assertion on the pointer.

**Tasks:**

- [ ] Add session storage enum or newtype to executor
- [ ] Implement `from_session_ptr()` constructor
- [ ] Ensure `drive_io()` and `spin_once()` work with borrowed session
- [ ] Add `session()` / `session_mut()` accessors that handle both variants
- [ ] Unit tests for borrowed-session executor lifecycle
- [ ] Document safety requirements in doc comments

**Files:** `nros-node/src/executor/spin.rs`

---

### 49.5 — nros-c Executor Migration

Rewrite `nros-c/src/executor.rs` to hold an opaque nros-node `Executor` in
the `_internal` field and delegate all operations.

**Delegation table:**

| C API function | Delegates to |
|----------------|-------------|
| `nano_ros_executor_init()` | `Executor::from_session_ptr()` |
| `nano_ros_executor_add_subscription()` | `executor.add_subscription_raw()` |
| `nano_ros_executor_add_timer()` | `executor.add_timer()` |
| `nano_ros_executor_add_service()` | `executor.add_service_raw()` |
| `nano_ros_executor_add_guard_condition()` | `executor.add_guard_condition()` |
| `nano_ros_executor_set_trigger()` | wraps C fn ptr as `Trigger::Predicate` |
| `nano_ros_executor_set_semantics()` | `executor.set_semantics()` |
| `nano_ros_executor_spin_some()` | `executor.spin_once()` |
| `nano_ros_executor_spin()` | loop over `executor.spin_once()` |
| `nano_ros_executor_spin_period()` | `executor.spin_period()` or manual drift-comp loop |
| `nano_ros_executor_spin_one_period()` | `executor.spin_one_period()` |

**What remains in nros-c:**

- Built-in triggers (`trigger_any` / `trigger_all` / `trigger_one` /
  `trigger_always`) — C-exported convenience functions
- `#[repr(C)]` struct definitions for `nano_ros_executor_t`
- Conversion between C enum values and Rust enums

**Concrete executor type:**

```rust
type CExecutor = nros_node::Executor<RmwSession, {MAX_HANDLES}, {ARENA_SIZE}>;
```

Constants come from `build.rs` (already exists: `NROS_EXECUTOR_MAX_HANDLES`,
arena size derived from handle/buffer counts).

**Expected reduction:** ~1,788 lines → ~400 lines. C header unchanged.

**Tasks:**

- [ ] Add `nros-node` as explicit dependency in `nros-c/Cargo.toml` (or
  access via `nros` unified crate)
- [ ] Define concrete `CExecutor` type alias with build-time constants
- [ ] Rewrite `nano_ros_executor_init()` to create `Executor::from_session_ptr()`
- [ ] Rewrite `nano_ros_executor_add_subscription()` to delegate to
  `add_subscription_raw()`
- [ ] Rewrite `nano_ros_executor_add_timer()` to delegate to `add_timer()`
- [ ] Rewrite `nano_ros_executor_add_service()` to delegate to
  `add_service_raw()`
- [ ] Rewrite `nano_ros_executor_add_guard_condition()` to delegate
- [ ] Rewrite `nano_ros_executor_set_trigger()` — wrap C fn ptr as
  `Trigger::Predicate`
- [ ] Rewrite `nano_ros_executor_set_semantics()` to delegate
- [ ] Rewrite `nano_ros_executor_spin_some()` to call `executor.spin_once()`
- [ ] Rewrite `nano_ros_executor_spin()` as loop over `spin_once()`
- [ ] Rewrite `nano_ros_executor_spin_period()` to delegate
- [ ] Rewrite `nano_ros_executor_spin_one_period()` to delegate
- [ ] Keep built-in trigger functions as C-exported wrappers
- [ ] Verify C header compatibility — no signature changes
- [ ] Remove self-implemented dispatch logic, handle arrays, LET buffers

**Files:** `nros-c/src/executor.rs`, `nros-c/Cargo.toml`

---

### 49.6 — nros-c Timer and Guard Condition Migration

**Timer:** Replace `nano_ros_timer_t._internal` with nros-node's Timer type.
Init, cancel, reset, call, and is_ready all delegate to Rust.

| C API function | Delegates to |
|----------------|-------------|
| `nano_ros_timer_init()` | `Timer::new()` |
| `nano_ros_timer_cancel()` | `timer.cancel()` |
| `nano_ros_timer_reset()` | `timer.reset()` |
| `nano_ros_timer_call()` | `timer.call()` |
| `nano_ros_timer_is_ready()` | `timer.is_ready()` |
| `nano_ros_timer_get_period()` | `timer.period()` |

**Expected reduction:** ~348 → ~150 lines.

**Guard condition:** Replace `nano_ros_guard_condition_t._internal` with
nros-node's `GuardCondition`. Trigger, clear, and callback all delegate.

| C API function | Delegates to |
|----------------|-------------|
| `nano_ros_guard_condition_init()` | `GuardCondition::new()` |
| `nano_ros_guard_condition_trigger()` | `guard.trigger()` |
| `nano_ros_guard_condition_clear()` | `guard.clear()` |
| `nano_ros_guard_condition_is_triggered()` | `guard.is_triggered()` |

**Expected reduction:** ~450 → ~150 lines.

**Tasks:**

- [ ] Rewrite `nano_ros_timer_init()` to create nros-node Timer
- [ ] Delegate `cancel()`, `reset()`, `call()`, `is_ready()`, `get_period()`
- [ ] Rewrite `nano_ros_guard_condition_init()` to create nros-node
  GuardCondition
- [ ] Delegate `trigger()`, `clear()`, `is_triggered()`
- [ ] Verify C header compatibility — no signature changes
- [ ] Remove self-implemented state machines from both files

**Files:** `nros-c/src/timer.rs`, `nros-c/src/guard_condition.rs`

---

### 49.7 — nros-c Action Migration

Rewrite action server and client to wrap nros-node's action types using
raw-bytes variants. Goal state machine, concurrent goal tracking, and
feedback publishing all delegate to nros-node.

**Prerequisites:** Raw-bytes action entry types in nros-node (similar to 49.1
but for action sub-services: goal request, cancel request, result request,
feedback publish, status publish).

**Delegation:**

| C API function | Delegates to |
|----------------|-------------|
| `nano_ros_action_server_init()` | `Executor::add_action_server_raw()` |
| `nano_ros_action_send_result()` | `action_server.send_result()` |
| `nano_ros_action_publish_feedback()` | `action_server.publish_feedback()` |
| `nano_ros_action_client_init()` | `Executor::add_action_client_raw()` |
| `nano_ros_action_send_goal()` | `action_client.send_goal()` |
| `nano_ros_action_get_result()` | `action_client.get_result()` |

**Expected reduction:** ~1,086 → ~400 lines.

**Tasks:**

- [ ] Add raw-bytes action server entry type to nros-node arena
- [ ] Add raw-bytes action client entry type to nros-node arena
- [ ] Implement raw-bytes dispatch for action sub-services
- [ ] Rewrite `nano_ros_action_server_init()` to delegate
- [ ] Rewrite `nano_ros_action_client_init()` to delegate
- [ ] Delegate goal state transitions to nros-node
- [ ] Delegate feedback publishing to nros-node
- [ ] Delegate result handling to nros-node
- [ ] Verify C header compatibility — no signature changes
- [ ] Remove self-implemented goal state machine and UUID tracking

**Files:** `nros-c/src/action.rs`, `nros-node/src/executor/action.rs`

---

### 49.8 — Tests and Verification

Validate that the migration preserves all existing behavior and add new
tests for nros-node capabilities added in this phase.

**Existing tests (must pass):**

- [ ] `just test-c` — all C API tests
- [ ] Zephyr C examples build and run (`just test-zephyr` C tests)
- [ ] Native C examples build and run
- [ ] `just quality` passes

**New nros-node unit tests:**

- [ ] Raw subscription callback dispatch (SubRawEntry)
- [ ] Raw service callback dispatch (SrvRawEntry)
- [ ] Guard condition trigger/clear/callback
- [ ] Guard condition executor integration
- [ ] LET semantics (data sampled once per cycle)
- [ ] LET semantics (default RclcppExecutor unchanged)
- [ ] Session-borrowing executor lifecycle
- [ ] Raw action server dispatch
- [ ] Raw action client dispatch

**Formal verification:**

- [ ] Kani harnesses for `GuardCondition` (trigger/clear atomicity)
- [ ] Kani harnesses for `ExecutorSemantics` (LET sampling correctness)
- [ ] Kani harnesses for raw-bytes entry types

---

## What Stays in nros-c (Not Migrated)

These modules are inherently C-specific — they handle `#[repr(C)]` struct
marshaling, raw CDR bytes, and direct RMW type creation that cannot be
wrapped around generic Rust types:

| Module | Lines | Reason |
|--------|------:|--------|
| `publisher.rs` | 549 | Init creates `RmwPublisher` directly (C has no generics) |
| `subscription.rs` | 465 | Init creates `RmwSubscriber` directly |
| `service.rs` | 838 | Init creates `RmwServiceServer`/`Client` directly |
| `cdr.rs` | 1,174 | Raw CDR serialization for C struct types |
| `parameter.rs` | 1,222 | Parameter server C bindings |
| `lifecycle.rs` | 728 | Lifecycle state machine C bindings |
| `node.rs` | 349 | Metadata container (`#[repr(C)]`) |
| `clock.rs` | 315 | Clock C bindings |
| `support.rs` | 283 | Session/support init |
| `platform.rs` | 183 | Platform abstraction C bindings |
| `qos.rs` | 122 | QoS profile C bindings |
| `lib.rs` | 85 | Module declarations |
| `error.rs` | 50 | Error code definitions |
| `constants.rs` | 28 | Build-time constants |
| `config.rs` | 6 | Config re-exports |

---

## Files to Create/Modify

| File | Changes |
|------|---------|
| `nros-node/src/executor/types.rs` | `RawSubscriptionCallback`, `RawServiceCallback`, `ExecutorSemantics`, `EntryKind::GuardCondition` |
| `nros-node/src/executor/arena.rs` | `SubRawEntry`, `SrvRawEntry`, `GuardConditionEntry`, `sub_raw_try_process()`, `srv_raw_try_process()`, `guard_try_process()`, `*_has_data()` fns, LET buffer fields |
| `nros-node/src/executor/spin.rs` | `add_subscription_raw()`, `add_service_raw()`, `add_guard_condition()`, `set_semantics()`, `from_session_ptr()`, LET pre-sample phase |
| `nros-node/src/executor/action.rs` | Raw-bytes action server/client entry types |
| `nros-node/src/executor/mod.rs` | Re-export new public types |
| `nros-node/src/guard_condition.rs` | New file: `GuardCondition` struct |
| `nros-node/src/lib.rs` | `pub mod guard_condition;` |
| `nros-c/src/executor.rs` | Rewrite to delegate to nros-node `Executor` |
| `nros-c/src/timer.rs` | Rewrite to delegate to nros-node `Timer` |
| `nros-c/src/guard_condition.rs` | Rewrite to delegate to nros-node `GuardCondition` |
| `nros-c/src/action.rs` | Rewrite to delegate to nros-node action types |
| `nros-c/Cargo.toml` | Ensure `nros-node` dependency (may already have via `nros`) |

---

## Verification

1. `just quality` — full format + clippy + nextest + miri + QEMU
2. `just test-c` — all C API tests pass unchanged
3. Zephyr C examples build and run
4. Native C examples build and run
5. New unit tests for raw-bytes dispatch, guard conditions, LET semantics
6. Kani bounded model checking on new types
7. Line count audit: migrated modules reduced by ~60%
