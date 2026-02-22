# Phase 49 — nros-c Thin Wrapper Migration

## Status: In Progress (49.1–49.12 complete)

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

Phase 47 (prerequisite) adds all nros-node executor infrastructure: trigger
conditions, `InvocationMode`, raw-bytes callbacks, guard conditions, LET
semantics, and session-borrowing executor. Phase 49 uses this complete
Rust API to rewrite nros-c as a thin delegation layer.

---

## Goals

1. **Rename C API prefix** — align `nano_ros_*` functions and types to `nros_*`,
   matching the naming convention used in Rust crates, Kconfig, and macros
2. **Eliminate duplicated executor logic** — nros-c delegates to nros-node's
   `Executor` for spin, trigger evaluation, handle management, and dispatch
3. **Add raw-bytes callback support to nros-node** — C callbacks receive
   `(*const u8, usize)`, not typed `&M`; nros-node must support this natively
4. **Add guard conditions to nros-node** — atomic signal type usable from
   any thread or ISR
5. **Add LET semantics to nros-node** — logical execution time: sample all
   subscriptions at spin start, process from snapshot
6. **Reduce nros-c line count** — target ~60% reduction in migrated modules

## Non-Goals

- Migrating C-specific modules (CDR marshaling, `#[repr(C)]` structs,
  publisher init) — these are inherently FFI-boundary code
- Adding new C API features — this phase is structural migration + naming
  alignment
- Removing the `#![allow(unsafe_op_in_unsafe_fn)]` attribute from nros-c

---

## Line Count Inventory

### Pre-migration (before Phase 49)

| Module               |      Lines | Category         | Notes                                      |
|----------------------|-----------:|------------------|--------------------------------------------|
| `executor.rs`        |      1,788 | Self-implemented | Dispatch, spin, triggers, LET, handle mgmt |
| `action.rs`          |      1,086 | Self-implemented | Goal state machine, UUID tracking          |
| `cdr.rs`             |      1,174 | C-specific       | Raw CDR serialization for C types          |
| `parameter.rs`       |      1,222 | C-specific       | Parameter server C bindings                |
| `service.rs`         |        838 | C-specific       | Service server/client init + raw dispatch  |
| `lifecycle.rs`       |        728 | C-specific       | Lifecycle state machine C bindings         |
| `publisher.rs`       |        549 | C-specific       | Publisher init + raw publish               |
| `subscription.rs`    |        465 | C-specific       | Subscription init + raw recv               |
| `guard_condition.rs` |        450 | Self-implemented | Atomic flag + callback                     |
| `node.rs`            |        349 | C-specific       | Metadata container                         |
| `timer.rs`           |        348 | Self-implemented | Period tracking + callback                 |
| `clock.rs`           |        315 | C-specific       | Clock C bindings                           |
| `support.rs`         |        283 | C-specific       | Session/support init                       |
| `platform.rs`        |        183 | C-specific       | Platform abstraction C bindings            |
| `qos.rs`             |        122 | C-specific       | QoS profile C bindings                     |
| `lib.rs`             |         85 | C-specific       | Module declarations                        |
| `error.rs`           |         50 | C-specific       | Error code definitions                     |
| `constants.rs`       |         28 | C-specific       | Build-time constants                       |
| `config.rs`          |          6 | C-specific       | Config re-exports                          |
| **Total**            | **10,069** |                  |                                            |

### Post-migration (after 49.1–49.3)

| Module               | Before | After | Delta | Notes                                        |
|----------------------|-------:|------:|------:|----------------------------------------------|
| `executor.rs`        |  1,788 | 1,437 |  -351 | Delegates to nros-node; 470 lines are tests  |
| `subscription.rs`    |    465 |   426 |   -39 | Metadata-only init, no RMW subscriber        |
| `service.rs`         |    838 |   793 |   -45 | Metadata-only init, no RMW service server    |
| `timer.rs`           |    348 |   395 |   +47 | Added `_executor` ptr, cancel/reset forward  |
| `guard_condition.rs` |    450 |   498 |   +48 | Added `GuardConditionHandle` storage         |
| `support.rs`         |    283 |   288 |    +5 | Added `get_session_ptr()` helper             |
| `action.rs`          |  1,086 | 1,291 |  +205 | Delegates to nros-node ActionServerRawHandle |
| Other modules        |  4,811 | 4,720 |   -91 | Minor rename changes                         |
| **Total**            | **10,069** | **9,643** | **-426** |                                   |

The net reduction is modest because (a) the executor gained ~470 lines of
tests and (b) timer/guard_condition grew slightly to add forwarding
infrastructure. The key improvement is **architectural**: dispatch logic now
lives in nros-node (shared with Rust API) rather than being duplicated in
nros-c. The self-implemented dispatch loop, handle arrays, LET buffers, and
trigger evaluation were all removed from executor.rs.

---

## Design Decisions

### Ownership Model: Session-Borrowing Executor

In the C API, the support object (`nros_support_t`) owns the session and
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
- CDR marshaling (`cdr.rs`)
- Parameter server bindings (`parameter.rs`)
- Lifecycle bindings (`lifecycle.rs`)
- Node metadata container (`node.rs`)
- All `#[repr(C)]` struct definitions
- QoS, clock, platform, error, constants modules

### Delegation Approach for Subscription/Service/Action Init

The key insight enabling delegation is that C init functions
(`nros_subscription_init()`, `nros_service_init()`, etc.) do NOT need to
create the RMW subscriber/server themselves. Instead:

1. **Init stores metadata only** — `nros_subscription_init()` saves the topic
   name, QoS settings, and type info into the `nros_subscription_t` struct,
   but does NOT create the RMW subscriber.
2. **Executor registration creates the RMW handle** —
   `nros_executor_add_subscription()` calls `executor.add_subscription_raw()`,
   which internally calls `session.create_subscriber()` AND registers the
   callback in the arena. The returned `HandleId` is stored in the subscription
   struct.
3. **No ownership conflict** — the nros-node `Executor` owns both the RMW
   subscriber and the callback entry in its arena. The C subscription struct
   is a lightweight metadata handle, not an owner.

This mirrors how `Node::create_subscription()` works in the Rust API: the
executor creates and owns the subscriber. The C API just defers this creation
from `init()` to `executor_add_*()`.

The same pattern applies to services and action servers/clients:
- `nros_service_init()` → stores metadata
- `nros_executor_add_service()` → calls `executor.add_service_raw()` → creates
  RMW service server + registers callback
- `nros_action_server_init()` → stores metadata
- `nros_executor_add_action_server()` → calls
  `executor.add_action_server_raw()` → creates sub-services + registers
  callbacks

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

## Prerequisites

Phase 49 requires the following nros-node Rust API to be complete before
any C API work begins:

| Prerequisite                                                                     | Phase | Sub-phase | Status   |
|----------------------------------------------------------------------------------|-------|-----------|----------|
| Trigger conditions (`Trigger` enum, `InvocationMode`, three-phase `spin_once()`) | 47    | 47.1–47.5 | Complete |
| Raw-bytes callbacks (`add_subscription_raw()`, `add_service_raw()`)              | 47    | 47.6      | Complete |
| Guard conditions (`add_guard_condition()`, `GuardConditionHandle`)               | 47    | 47.7      | Complete |
| LET semantics (pre-sample phase in `spin_once()`)                                | 47    | 47.8      | Complete |
| Session-borrowing executor (`from_session_ptr()`, `SessionStore`)                | 47    | 47.9      | Complete |

All Rust executor infrastructure is implemented in Phase 47. Phase 49 is
purely C API work: rename the `nano_ros_` prefix and rewrite nros-c modules
to delegate to nros-node.

---

## Sub-phases

### 49.1 — C API Prefix Rename (`nano_ros_` → `nros_`) — COMPLETE

Renamed all C-facing code-level identifiers from the `nano_ros_` prefix to
`nros_`, aligning the C API with the code-level naming convention used
everywhere else (Rust crate names, Kconfig symbols, header directory, macros).
This was done **before** the delegation migration (49.2–49.4) so that the new
thin-wrapper code uses final names from the start.

**What was renamed (code-level identifiers):**

| Category          | From                             | To                           | Count |
|-------------------|----------------------------------|------------------------------|------:|
| C functions       | `nano_ros_support_init()`        | `nros_support_init()`        |  ~142 |
| C types           | `nano_ros_publisher_t`           | `nros_publisher_t`           |   ~46 |
| Rust FFI          | `#[unsafe(no_mangle)]` fn names | matching `nros_*`            |  ~142 |
| Codegen templates | `nano_ros_cdr_*` calls           | `nros_cdr_*`                 |    ~6 |
| Codegen Rust      | `NanoRosField`, etc.             | `NrosField`, etc.            |   ~20 |
| C examples        | 14 `main.c` files                | `nros_*` API calls           |    14 |
| CLAUDE.md         | naming convention section        | updated                      |     1 |
| Docs              | various .md files                | updated                      |   ~10 |

**What was kept (project-level names):**
- CMake target: `NanoRos::NanoRos` (project name, not code abbreviation)
- CMake variable: `NANO_ROS_RMW`
- CMake function: `nano_ros_generate_interfaces()`
- CMake config: `NanoRosConfig.cmake`
- CMake build dirs: `nano_ros_c/`, `__nano_ros_c` suffixes
- CLI: `cargo nano-ros`
- Kconfig symbols: already `CONFIG_NROS_*`
- C macros: already `NROS_*`
- Header directory: already `nros/`

**Completed tasks:**

- [x] Rename all `nano_ros_*()` functions to `nros_*()` in C headers
- [x] Rename all `nano_ros_*_t` types to `nros_*_t` in C headers
- [x] Update all `#[unsafe(no_mangle)]` function names in nros-c Rust source
- [x] Update all `#[repr(C)]` struct names to match renamed typedefs
- [x] Update codegen C templates to emit `nros_cdr_*` and `nros_*_type_t`
- [x] Rename codegen Rust identifiers (`NrosField`, `NrosCodegenMode`, etc.)
- [x] Rename codegen template files (`*_nano_ros.*` → `*_nros.*`)
- [x] Update all C example `main.c` files
- [x] Update CLAUDE.md naming convention section
- [x] Update all documentation references

---

### 49.2 — nros-c Executor Migration — COMPLETE

Rewrite `nros-c/src/executor.rs` to hold an opaque nros-node `Executor` in
the `_internal` field and delegate all operations.

**Delegation approach:**

The executor struct `nros_executor_t._internal` stores a
`*mut Executor<RmwSession, MAX_HANDLES, ARENA_SIZE>`. All C API functions
dereference this pointer and delegate.

For subscriptions and services, the C init function (`nros_subscription_init`)
stores metadata only (topic name, QoS, type info) — it does NOT create the
RMW subscriber. The executor registration function
(`nros_executor_add_subscription`) calls `executor.add_subscription_raw()`,
which creates the RMW subscriber AND registers the callback in the arena.
The returned `HandleId` is stored in the C subscription struct. This mirrors
how `Node::create_subscription()` works in the Rust API and eliminates the
ownership conflict where subscribers would otherwise be created outside the
executor.

**Delegation table:**

| C API function (post-rename)              | Delegates to                                       |
|-------------------------------------------|----------------------------------------------------|
| `nros_executor_init()`                    | `Executor::from_session_ptr()`                     |
| `nros_executor_add_subscription()`        | `executor.add_subscription_raw()`                  |
| `nros_executor_add_timer()`               | `executor.add_timer()`                             |
| `nros_executor_add_service()`             | `executor.add_service_raw()`                       |
| `nros_executor_add_guard_condition()`     | `executor.add_guard_condition()`                   |
| `nros_executor_set_trigger()`             | wraps C fn ptr as `Trigger::Predicate`             |
| `nros_executor_set_semantics()`           | `executor.set_semantics()`                         |
| `nros_executor_spin_some()`               | `executor.spin_once()`                             |
| `nros_executor_spin()`                    | loop over `executor.spin_once()`                   |
| `nros_executor_spin_period()`             | `executor.spin_period()` or manual drift-comp loop |
| `nros_executor_spin_one_period()`         | `executor.spin_one_period()`                       |

**Subscription/service init changes:**

```c
// Before: nros_subscription_init() created RMW subscriber in _internal
// After:  nros_subscription_init() stores topic, qos, type_info only
//         nros_executor_add_subscription() creates subscriber + registers callback
```

The same pattern applies to services and action servers/clients.

**What remains in nros-c:**

- Built-in triggers (`trigger_any` / `trigger_all` / `trigger_one` /
  `trigger_always`) — C-exported convenience functions
- `#[repr(C)]` struct definitions for `nros_executor_t`
- Conversion between C enum values and Rust enums
- `nros_subscription_init()` / `nros_service_init()` — simplified to metadata
  storage only (no RMW handle creation)

**Concrete executor type:**

```rust
type CExecutor = nros_node::Executor<RmwSession, {MAX_HANDLES}, {ARENA_SIZE}>;
```

Constants come from `build.rs` (already exists: `NROS_EXECUTOR_MAX_HANDLES`,
arena size derived from handle/buffer counts).

**Expected reduction:** ~1,788 lines → ~400 lines.

**Tasks:**

- [x] Add `nros-node` as explicit dependency in `nros-c/Cargo.toml`
- [x] Define concrete `CExecutor` type alias with build-time constants
- [x] Rewrite `nros_executor_init()` to create `Executor::from_session_ptr()`
- [x] Refactor `nros_subscription_init()` to store metadata only (no RMW
  subscriber creation)
- [x] Rewrite `nros_executor_add_subscription()` to call
  `executor.add_subscription_raw_with_qos_sized()` — creates subscriber +
  registers callback
- [x] Refactor `nros_service_init()` to store metadata only
- [x] Rewrite `nros_executor_add_service()` to call
  `executor.add_service_raw_sized()` — creates service + registers callback
- [x] Rewrite `nros_executor_add_timer()` to delegate to `add_timer()`
- [x] Rewrite `nros_executor_add_guard_condition()` to delegate
- [x] Rewrite `nros_executor_set_trigger()` — wrap C fn ptr as
  `Trigger::RawPredicate`
- [x] Rewrite `nros_executor_set_semantics()` to delegate
- [x] Rewrite `nros_executor_spin_some()` to call `executor.spin_once()`
- [x] Rewrite `nros_executor_spin()` as loop over `spin_once()`
- [x] Rewrite `nros_executor_spin_period()` to delegate
- [x] Rewrite `nros_executor_spin_one_period()` to delegate
- [x] Keep built-in trigger functions as C-exported wrappers
- [x] Remove self-implemented dispatch logic, handle arrays, LET buffers

**nros-node API extensions (added for 49.2):**

- [x] `add_subscription_raw_with_qos_sized::<RX_BUF>()` — QoS + const-generic
  buffer for raw subscription registration
- [x] `add_service_raw_sized::<REQ_BUF, RESP_BUF>()` — const-generic buffer
  for raw service registration
- [x] `Trigger::RawPredicate { callback, context }` — bridges C trigger API
  (`fn(*const bool, usize, *mut c_void) -> bool`) to nros-node triggers
- [x] Timer control methods: `cancel_timer()`, `reset_timer()`,
  `timer_is_cancelled()`, `timer_period_ms()` on `Executor`
- [x] `HandleId.0` changed to `pub` for cross-crate access

**Line count:** executor.rs 1,788 → 1,437 lines (includes 470 lines of
tests). The executor struct lost handle arrays and LET buffers; all dispatch
logic delegates to nros-node.

**Files:** `nros-c/src/executor.rs`, `nros-c/src/subscription.rs`,
`nros-c/src/service.rs`, `nros-c/Cargo.toml`,
`nros-node/src/executor/spin.rs`, `nros-node/src/executor/types.rs`

---

### 49.3 — nros-c Timer and Guard Condition Migration — COMPLETE

**Timer:** Timer init stores metadata (period, callback, context). The
executor creates the nros-node timer entry via `add_timer()`. Cancel and
reset forward to the nros-node executor via a stored `_executor` pointer.
`is_ready()`, `call()`, and `get_period()` are kept as local state queries
(they don't need executor delegation since the executor drives readiness
internally).

| C API function (post-rename) | Implementation                                  |
|------------------------------|--------------------------------------------------|
| `nros_timer_init()`          | Stores metadata (unchanged — already metadata-only) |
| `nros_timer_cancel()`        | Forwards to `executor.cancel_timer(handle_id)`   |
| `nros_timer_reset()`         | Forwards to `executor.reset_timer(handle_id)`    |
| `nros_timer_call()`          | Local (executor drives callbacks internally)     |
| `nros_timer_is_ready()`      | Local (executor handles readiness internally)    |
| `nros_timer_get_period()`    | Local (reads `period_ns` field)                  |

**Line count:** timer.rs 348 → 395 lines (added `_executor` field, helper
methods, and forwarding logic; slight increase due to `#[cfg(feature)]`
guards).

**Guard condition:** Guard condition init stores metadata. The executor
creates a `GuardConditionHandle` (containing `*const AtomicBool` from the
arena) via `add_guard_condition()`, and stores it in the guard struct.
`trigger()` uses the handle's atomic flag for thread-safe signaling.
`fini()` drops the boxed handle.

| C API function (post-rename)          | Implementation                              |
|---------------------------------------|---------------------------------------------|
| `nros_guard_condition_init()`         | Stores metadata (unchanged)                 |
| `nros_guard_condition_trigger()`      | `guard_handle.trigger()` (AtomicBool)       |
| `nros_guard_condition_clear()`        | Local atomic store (fallback flag)          |
| `nros_guard_condition_is_triggered()` | Local atomic load (fallback flag)           |
| `nros_guard_condition_fini()`         | Drops boxed `GuardConditionHandle`          |

**Line count:** guard_condition.rs 450 → 498 lines (added `handle_id`,
`_guard_handle` fields, `set_guard_handle()`/`get_guard_handle()` methods;
slight increase due to alloc-gated methods).

**Tasks:**

- [x] Add `_executor` pointer to `nros_timer_t`, set during
  `nros_executor_add_timer()`
- [x] `nros_timer_cancel()` forwards to `executor.cancel_timer(handle_id)`
- [x] `nros_timer_reset()` forwards to `executor.reset_timer(handle_id)`
- [x] `nros_timer_fini()` clears executor pointer and handle ID
- [x] Add `_guard_handle` field to `nros_guard_condition_t` for
  `GuardConditionHandle` storage
- [x] `nros_guard_condition_trigger()` uses guard handle when registered
- [x] `nros_guard_condition_fini()` drops boxed guard handle
- [x] All 76 nros-c unit tests pass
- [x] Both rmw-zenoh and rmw-xrce backends compile cleanly

**Files:** `nros-c/src/timer.rs`, `nros-c/src/guard_condition.rs`,
`nros-c/src/executor.rs` (CExecutor made `pub(crate)`)

---

### 49.4 — nros-c Action Server Migration — COMPLETE

Rewrote the action server module to delegate to nros-node's
`ActionServerRawHandle` and `Executor::add_action_server_raw()` API. Follows
the same metadata-only init → executor registration pattern as subscriptions
(49.2) and services (49.2).

**Pattern:**

1. `nros_action_server_init()` stores metadata (name, type, callbacks) only
2. `nros_executor_add_action_server()` creates `Box<ActionServerInternal>`,
   registers with nros-node via `add_action_server_raw_sized()`, stores the
   returned `ActionServerRawHandle`
3. Operation functions (`publish_feedback`, `succeed`, `abort`, `canceled`,
   `execute`) delegate through the handle

**Callback trampolines:**

Two trampolines bridge C and Rust callback ABIs:

- `goal_callback_trampoline`: Wraps C `nros_goal_callback_t` as
  `RawGoalCallback`. On acceptance, fills a C-side goal slot and calls the
  accepted callback. GoalResponse enum values match (0=Reject, 1=Execute,
  2=Defer).
- `cancel_callback_trampoline`: Wraps C `nros_cancel_callback_t` as
  `RawCancelCallback`. Maps inverted response codes (C: REJECT=0/ACCEPT=1
  vs Rust: Ok=0/Rejected=1). Finds matching C-side goal slot by UUID.

**Delegation table:**

| C API function                            | Delegates to                                |
|-------------------------------------------|---------------------------------------------|
| `nros_action_server_init()`               | Metadata storage only                       |
| `nros_executor_add_action_server()`       | `executor.add_action_server_raw_sized()`    |
| `nros_action_publish_feedback()`          | `handle.publish_feedback_raw()`             |
| `nros_action_succeed()`                   | `handle.complete_goal_raw(Succeeded)`       |
| `nros_action_abort()`                     | `handle.complete_goal_raw(Aborted)`         |
| `nros_action_canceled()`                  | `handle.complete_goal_raw(Canceled)`        |
| `nros_action_execute()`                   | `handle.set_goal_status(Executing)`         |
| `nros_action_server_get_active_goal_count()` | C-side counter (maintained by trampolines) |
| `nros_action_server_fini()`               | Drops `Box<ActionServerInternal>`           |

**What stays as stubs:** Client functions (`send_goal`, `cancel_goal`,
`get_result`) remain stubs — no `add_action_client_raw` on executor yet.

**Line count:** action.rs 1,086 → 1,291 lines (added `ActionServerInternal`,
trampolines, and delegation logic).

**Tasks:**

- [x] Add `ActionServerInternal` struct (handle, executor_ptr, C callbacks)
- [x] Add `goal_callback_trampoline` (wraps C goal callback as RawGoalCallback)
- [x] Add `cancel_callback_trampoline` (wraps C cancel callback as
  RawCancelCallback, maps inverted response codes)
- [x] Simplify `nros_action_server_init()` to metadata-only
- [x] Add `nros_executor_add_action_server()` to executor.rs
- [x] Add C header declaration for `nros_executor_add_action_server()`
- [x] Rewrite `nros_action_publish_feedback()` → `handle.publish_feedback_raw()`
- [x] Rewrite `nros_action_succeed/abort/canceled()` → `handle.complete_goal_raw()`
- [x] Rewrite `nros_action_execute()` → `handle.set_goal_status(Executing)`
- [x] Update `nros_action_server_fini()` to drop Box
- [x] Fix clippy warnings (collapsible_if)
- [x] All 76 nros-c unit tests pass
- [x] Build + clippy clean

**Files:** `nros-c/src/action.rs`, `nros-c/src/executor.rs`,
`nros-c/include/nros/executor.h`

---

### 49.5 — Tests and Verification — COMPLETE (for 49.1–49.4)

All existing tests pass.

**Existing tests (must pass):**

- [x] `just test-c` — all 15 C API tests pass (including action tests)
- [ ] Zephyr C examples build and run (`just test-zephyr` C tests) — not
  yet re-tested
- [x] Native C examples build and run (verified via `test-c`)
- [x] `just quality` passes (format, clippy, nextest, miri, QEMU)

**nros-node unit tests (from Phase 47):**

- [x] Raw subscription callback dispatch (SubRawEntry) — 86 tests in
  nros-node executor
- [x] Raw service callback dispatch (SrvRawEntry)
- [x] Guard condition trigger/clear/callback
- [x] Guard condition executor integration
- [x] LET semantics (data sampled once per cycle)
- [x] LET semantics (default RclcppExecutor unchanged)
- [x] Session-borrowing executor lifecycle

**nros-c unit tests:**

- [x] 76 nros-c unit tests pass (executor, timer, guard condition,
  subscription, service, lifecycle, parameter, platform, etc.)
- [x] Both rmw-zenoh and rmw-xrce backends compile cleanly with no warnings

**Future (not blocking):**

- [ ] Raw action client dispatch (needs `add_action_client_raw`)
- [ ] Kani harnesses for `GuardCondition` (trigger/clear atomicity)
- [ ] Kani harnesses for `ExecutorSemantics` (LET sampling correctness)
- [ ] Kani harnesses for raw-bytes entry types

---

## What Stays in nros-c (Not Migrated)

These modules are inherently C-specific — they handle `#[repr(C)]` struct
marshaling, raw CDR bytes, and metadata storage:

| Module            | Lines | Reason                                                         |
|-------------------|------:|----------------------------------------------------------------|
| `publisher.rs`    |   549 | Init creates `RmwPublisher` directly (C has no generics)       |
| `subscription.rs` |   465 | Simplified to metadata storage; RMW creation moves to executor |
| `service.rs`      |   838 | Simplified to metadata storage; RMW creation moves to executor |
| `cdr.rs`          | 1,174 | Raw CDR serialization for C struct types                       |
| `parameter.rs`    | 1,222 | Parameter server C bindings                                    |
| `lifecycle.rs`    |   728 | Lifecycle state machine C bindings                             |
| `node.rs`         |   349 | Metadata container (`#[repr(C)]`)                              |
| `clock.rs`        |   315 | Clock C bindings                                               |
| `support.rs`      |   283 | Session/support init                                           |
| `platform.rs`     |   183 | Platform abstraction C bindings                                |
| `qos.rs`          |   122 | QoS profile C bindings                                         |
| `lib.rs`          |    85 | Module declarations                                            |
| `error.rs`        |    50 | Error code definitions                                         |
| `constants.rs`    |    28 | Build-time constants                                           |
| `config.rs`       |     6 | Config re-exports                                              |

Note: `subscription.rs` and `service.rs` are simplified during 49.2 — their
init functions change from "create RMW handle" to "store metadata". The RMW
handle creation moves into the executor registration path, which delegates to
nros-node.

---

## Files to Create/Modify

**49.1 (C API rename):**

| File                                 | Changes                                     |
|--------------------------------------|---------------------------------------------|
| `nros-c/include/nros/*.h` (20 files) | Rename `nano_ros_*` → `nros_*` in all decls |
| `nros-c/src/*.rs` (19 files)         | Rename `nano_ros_*` → `nros_*` in FFI fns   |
| `CMakeLists.txt`                     | `NANO_ROS_RMW` → `NROS_RMW`                 |
| `cmake/*.cmake`                      | `NanoRos` → `Nros` in targets and configs   |
| `zephyr/CMakeLists.txt`              | Update target names                         |
| `zephyr/cmake/*.cmake`               | Update function/target names                |
| `nano-ros-codegen-c/`                | Emit `nros_*` names in generated code       |
| C example `main.c` files (14)        | Update all API calls                        |
| `CLAUDE.md`                          | Update naming convention section            |
| Various docs (~10 .md files)         | Update references                           |

**49.2–49.4 (nros-c delegation):**

| File                            | Changes                                                       |
|---------------------------------|---------------------------------------------------------------|
| `nros-c/src/executor.rs`        | Rewrite to delegate to nros-node `Executor`                   |
| `nros-c/src/subscription.rs`    | Simplify to metadata storage (RMW creation moves to executor) |
| `nros-c/src/service.rs`         | Simplify to metadata storage (RMW creation moves to executor) |
| `nros-c/src/timer.rs`           | Rewrite to delegate to nros-node `Timer`                      |
| `nros-c/src/guard_condition.rs` | Rewrite to delegate to nros-node `GuardCondition`             |
| `nros-c/src/action.rs`          | Rewrite to delegate to nros-node action types                 |
| `nros-c/Cargo.toml`             | Ensure `nros-node` dependency                                 |

---

## Verification

1. [x] `just quality` — full format + clippy + nextest + miri + QEMU — PASSES
2. [x] `just test-c` — all 15 C API tests pass unchanged
3. [ ] Zephyr C examples build and run — not yet re-tested
4. [x] Native C examples build and run (via test-c)
5. [x] nros-node unit tests for raw-bytes dispatch, guard conditions, LET
   semantics — 86 tests from Phase 47
6. [ ] Kani bounded model checking on new types — deferred with 49.4
7. [x] Line count audit — see Post-migration table above

---

### 49.6 — Wire C Action Client — COMPLETE

Wired the action client stubs (`nros_action_send_goal`, `nros_action_cancel_goal`,
`nros_action_get_result`) to actually communicate over the transport, and added
a new `nros_action_try_recv_feedback()` function for non-blocking feedback polling.

**Design:** Unlike the action server (which uses executor arena via
`add_action_server_raw()`), the action client creates RMW entities during init
(like the service client). `nros_action_client_init()` accesses the session via
`node_ref.get_support_mut()` → `get_session_mut()`, creates 3 service clients
(send_goal, cancel_goal, get_result) and 1 feedback subscriber, then constructs
an `ActionClientCore` stored as `Box<ActionClientInternal>` in `_internal`.

**Key struct:**

```rust
struct ActionClientInternal {
    core: ActionClientCore<RmwServiceClient, RmwSubscriber, MSG_BUF, MSG_BUF, MSG_BUF>,
}
```

**Delegation table:**

| C API function                    | Delegates to                                    |
|-----------------------------------|-------------------------------------------------|
| `nros_action_client_init()`       | Creates `ActionClientCore` via session           |
| `nros_action_send_goal()`         | `core.send_goal_raw(goal_data)`                  |
| `nros_action_cancel_goal()`       | `core.send_cancel_request(goal_id)`              |
| `nros_action_get_result()`        | `core.send_get_result_request()` + poll loop     |
| `nros_action_try_recv_feedback()` | `core.try_recv_feedback_raw()` + callback        |
| `nros_action_client_fini()`       | Drops `Box<ActionClientInternal>`                |

**New C API function:** `nros_action_try_recv_feedback(client)` — non-blocking
feedback poll. If feedback is available, invokes the feedback callback (if set).
Returns `NROS_RET_OK` if feedback dispatched, `NROS_RET_TIMEOUT` if none.

**Tasks:**

- [x] Add `ActionClientInternal` struct wrapping `ActionClientCore`
- [x] Wire `nros_action_client_init()` — create 3 service clients + 1 subscriber
- [x] Wire `nros_action_send_goal()` — `core.send_goal_raw()`
- [x] Wire `nros_action_cancel_goal()` — `core.send_cancel_request()`
- [x] Wire `nros_action_get_result()` — send request + poll loop
- [x] Add `nros_action_try_recv_feedback()` — non-blocking feedback poll
- [x] Wire `nros_action_client_fini()` — drop Box
- [x] Add C header declaration for `nros_action_try_recv_feedback()`
- [x] `cargo clippy -p nros-c` passes clean

**Files:** `nros-c/src/action.rs`, `nros-c/include/nros/action.h`

---

### 49.7 — Native C XRCE Service Examples — COMPLETE

Created 2 native C examples for service server and client using XRCE-DDS
transport. The C API is RMW-agnostic — the only difference from zenoh examples
is the locator string and build-time RMW selection (`-DNANO_ROS_RMW=xrce`).

**Examples created:**

| Example | Directory |
|---------|-----------|
| Service server | `examples/native/c/xrce/service-server/` |
| Service client | `examples/native/c/xrce/service-client/` |

Each has: `CMakeLists.txt`, `src/main.c`, `.gitignore`

**CMake pattern:** `nano_ros_generate_interfaces(example_interfaces "srv/AddTwoInts.srv" SKIP_INSTALL)`

**Tasks:**

- [x] Create `examples/native/c/xrce/service-server/` (CMake, main.c, .gitignore)
- [x] Create `examples/native/c/xrce/service-client/` (CMake, main.c, .gitignore)

---

### 49.8 — Native C XRCE Action Examples — COMPLETE

Created 2 native C examples for action server and client using XRCE-DDS
transport. Action server includes `nros_executor_add_action_server()`.
Action client uses generated types for Fibonacci goal/result/feedback.

**Examples created:**

| Example | Directory |
|---------|-----------|
| Action server | `examples/native/c/xrce/action-server/` |
| Action client | `examples/native/c/xrce/action-client/` |

**Tasks:**

- [x] Create `examples/native/c/xrce/action-server/` (CMake, main.c, .gitignore)
- [x] Create `examples/native/c/xrce/action-client/` (CMake, main.c, .gitignore)

---

### 49.9 — Zephyr C Service Examples (Zenoh + XRCE) — COMPLETE

Created 4 Zephyr C examples for service server and client using both Zenoh
and XRCE-DDS transports.

**Examples created:**

| Example | Directory |
|---------|-----------|
| Zenoh service server | `examples/zephyr/c/zenoh/service-server/` |
| Zenoh service client | `examples/zephyr/c/zenoh/service-client/` |
| XRCE service server  | `examples/zephyr/c/xrce/service-server/`  |
| XRCE service client  | `examples/zephyr/c/xrce/service-client/`  |

Each has: `CMakeLists.txt`, `prj.conf`, `src/main.c`

**Zephyr patterns:**
- Zenoh: `zpico_zephyr_wait_network()`, `CONFIG_NROS_ZENOH_LOCATOR`
- XRCE: `xrce_zephyr_wait_network()` + `xrce_zephyr_init()`, `CONFIG_NROS_XRCE_AGENT_ADDR`

**Tasks:**

- [x] Create 4 Zephyr service examples (zenoh/xrce × server/client)

---

### 49.10 — Zephyr C Action Examples (Zenoh + XRCE) — COMPLETE

Created 4 Zephyr C examples for action server and client using both Zenoh
and XRCE-DDS transports. Action servers include `nros_executor_add_action_server()`.

**Examples created:**

| Example | Directory |
|---------|-----------|
| Zenoh action server | `examples/zephyr/c/zenoh/action-server/` |
| Zenoh action client | `examples/zephyr/c/zenoh/action-client/` |
| XRCE action server  | `examples/zephyr/c/xrce/action-server/`  |
| XRCE action client  | `examples/zephyr/c/xrce/action-client/`  |

**Tasks:**

- [x] Create 4 Zephyr action examples (zenoh/xrce × server/client)

---

### 49.11 — Kani Verification for service.rs — COMPLETE

Added 13 Kani bounded model checking harnesses for `service.rs`, covering
service server and client null-pointer safety, state validation, and
double-init rejection.

**Service server harnesses (6):**
- `service_init_null_ptrs` — NULL for each of 6 params → `NROS_RET_INVALID_ARGUMENT`
- `service_init_none_callback` — `callback: None` → `NROS_RET_INVALID_ARGUMENT`
- `service_init_uninit_node` — uninitialized node → `NROS_RET_NOT_INIT`
- `service_zero_initialized_state` — default state is UNINITIALIZED
- `service_fini_null_safety` — NULL → `NROS_RET_INVALID_ARGUMENT`
- `service_double_init_rejected` — re-init → `NROS_RET_BAD_SEQUENCE`

**Service client harnesses (5):**
- `client_init_null_ptrs` — NULL for each of 4 params
- `client_init_uninit_node` — uninitialized node → `NROS_RET_NOT_INIT`
- `client_zero_initialized_state` — default state
- `client_fini_null_safety` — NULL and wrong-state checks
- `client_call_null_safety` — NULL for each of 6 params of `nros_client_call`

**Utility harnesses (2):**
- `service_name_getter_null` — NULL → NULL
- `client_name_getter_null` — NULL → NULL

**Files:** `nros-c/src/service.rs`

---

### 49.12 — Kani Verification for action.rs — COMPLETE

Added 20 Kani bounded model checking harnesses for `action.rs`, covering
action server/client null-pointer safety, state validation, goal handle
operations, and UUID utilities.

**Action server harnesses (8):**
- `action_server_init_null_ptrs` — NULL for each of 8 params
- `action_server_init_none_goal_callback` — `goal_callback: None` → `INVALID_ARGUMENT`
- `action_server_init_uninit_node` — uninitialized node → `NOT_INIT`
- `action_server_zero_initialized_state` — default state, all pointers null
- `action_server_fini_null_safety` — NULL and wrong-state checks
- `action_server_double_init_rejected` — `BAD_SEQUENCE` on re-init
- `action_server_active_goal_count_null` — NULL → 0
- `action_publish_feedback_null_ptrs` — NULL goal/feedback → `INVALID_ARGUMENT`

**Action client harnesses (6):**
- `action_client_init_null_ptrs` — NULL for each of 4 params
- `action_client_init_uninit_node` — `NOT_INIT`
- `action_client_zero_initialized_state` — default state
- `action_client_fini_null_safety` — NULL and wrong-state
- `action_send_goal_null_ptrs` — NULL for each of 4 params
- `action_get_result_null_ptrs` — NULL for each of 5 params

**Goal handle harnesses (3):**
- `goal_succeed_null_ptr` — NULL → `INVALID_ARGUMENT`
- `goal_abort_null_ptr` — NULL → `INVALID_ARGUMENT`
- `goal_canceled_null_ptr` — NULL → `INVALID_ARGUMENT`

**UUID / Utility harnesses (3):**
- `goal_uuid_generate_null` — NULL → `INVALID_ARGUMENT`
- `goal_uuid_equal_null` — NULL → false
- `goal_status_to_string_all_variants` — all 7 variants → non-null

**Files:** `nros-c/src/action.rs`

---

## Example Coverage Matrix (after 49.7–49.10)

| Platform | RMW   | talker | listener | service-server | service-client | action-server | action-client |
|----------|-------|:------:|:--------:|:--------------:|:--------------:|:-------------:|:-------------:|
| Native   | Zenoh | YES    | YES      | YES            | YES            | YES           | YES           |
| Native   | XRCE  | YES    | YES      | YES            | YES            | YES           | YES           |
| Zephyr   | Zenoh | YES    | YES      | YES            | YES            | YES           | YES           |
| Zephyr   | XRCE  | YES    | YES      | YES            | YES            | YES           | YES           |

---

## Remaining Work

### Immediate (no blockers)

- **Zephyr C example re-test**: Run `just test-zephyr` to verify C examples
  still build and run on Zephyr native_sim. Expected to pass since the C API
  signature is unchanged (only internal delegation changed).

### Not Planned

- **Publisher migration**: Publisher init creates `RmwPublisher` directly,
  which is appropriate for the C API (no generics to delegate)
- **CDR, parameter, lifecycle, node, clock, platform, qos, error modules**:
  These are inherently C-specific and don't need delegation
