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

## Current State Inventory

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

| Prerequisite | Phase | Sub-phase | Status |
|--------------|-------|-----------|--------|
| Trigger conditions (`Trigger` enum, `InvocationMode`, three-phase `spin_once()`) | 47 | 47.1–47.5 | Not Started |
| Raw-bytes callbacks (`add_subscription_raw()`, `add_service_raw()`) | 47 | 47.6 | Not Started |
| Guard conditions (`add_guard_condition()`, `GuardConditionHandle`) | 47 | 47.7 | Not Started |
| LET semantics (pre-sample phase in `spin_once()`) | 47 | 47.8 | Not Started |
| Session-borrowing executor (`from_session_ptr()`, `SessionStore`) | 47 | 47.9 | Not Started |

All Rust executor infrastructure is implemented in Phase 47. Phase 49 is
purely C API work: rename the `nano_ros_` prefix and rewrite nros-c modules
to delegate to nros-node.

---

## Sub-phases

### 49.1 — C API Prefix Rename (`nano_ros_` → `nros_`)

Rename all C-facing items from the `nano_ros_` prefix to `nros_`, aligning
the C API with the code-level naming convention used everywhere else (Rust
crate names, Kconfig symbols, header directory, macros). This must happen
**before** the delegation migration (49.2–49.4) so that the new thin-wrapper
code is written with the final names from the start.

**Current state:**
- **Functions**: 142 `nano_ros_*()` declarations across 20 headers
- **Types**: 46 `nano_ros_*_t` typedefs (structs/enums)
- **Exception**: `nros_node_t` already uses `nros_` prefix
- **Macros**: Already use `NROS_` prefix (no change needed)
- **Headers**: Already in `nros/` directory (no change needed)
- **CMake**: `NanoRos::NanoRos` target, `NANO_ROS_RMW` variable,
  `nano_ros_generate_interfaces()` function, `NanoRosConfig.cmake`

**Rename scope:**

| Category | From | To | Count |
|----------|------|----|------:|
| C functions | `nano_ros_support_init()` | `nros_support_init()` | ~142 |
| C types | `nano_ros_publisher_t` | `nros_publisher_t` | ~46 |
| CMake target | `NanoRos::NanoRos` | `Nros::Nros` | 1 |
| CMake variable | `NANO_ROS_RMW` | `NROS_RMW` | 1 |
| CMake function | `nano_ros_generate_interfaces()` | `nros_generate_interfaces()` | 1 |
| CMake config | `NanoRosConfig.cmake` | `NrosConfig.cmake` | 1 |
| Codegen binary | `nros-codegen` (unchanged) | — | 0 |
| Build dir | `build/nano_ros_c/` | `build/nros_c/` | 1 |
| Generated targets | `std_msgs__nano_ros_c` | `std_msgs__nros_c` | ~5 |
| CLAUDE.md | naming convention section | update | 1 |
| Docs | various .md files | update | ~10 |
| C examples | 14 `main.c` files | update | 14 |
| Zephyr CMake | `zephyr/CMakeLists.txt` | update | 1 |

**NOT renamed:**
- `cargo nano-ros` CLI command (user-facing tool name, kept for clarity)
- Kconfig symbols (already `CONFIG_NROS_*`)
- C macros (already `NROS_*`)
- Header directory (already `nros/`)

**Tasks:**

- [ ] Rename all `nano_ros_*()` functions to `nros_*()` in C headers
  (`packages/core/nros-c/include/nros/*.h`)
- [ ] Rename all `nano_ros_*_t` types to `nros_*_t` in C headers
- [ ] Update all `#[unsafe(no_mangle)]` function names in nros-c Rust source
  (`packages/core/nros-c/src/*.rs`)
- [ ] Update all `#[repr(C)]` struct names to match renamed typedefs
- [ ] Rename CMake target `NanoRos::NanoRos` → `Nros::Nros` and update
  config files (`NanoRosConfig.cmake` → `NrosConfig.cmake`, etc.)
- [ ] Rename CMake variable `NANO_ROS_RMW` → `NROS_RMW`
- [ ] Rename CMake function `nano_ros_generate_interfaces()` →
  `nros_generate_interfaces()`
- [ ] Update codegen C backend to emit `nros_*` names
  (`packages/codegen/packages/nano-ros-codegen-c/`)
- [ ] Update all C example `main.c` files (14 files in
  `examples/native/c/` and `examples/zephyr/c/`)
- [ ] Update `zephyr/CMakeLists.txt` and `zephyr/cmake/` modules
- [ ] Update CLAUDE.md naming convention section
- [ ] Update all documentation references (~10 .md files)
- [ ] Add backward-compat `#define` aliases in a single
  `nros/compat.h` header (optional, can be removed in a future release)
- [ ] `just quality` passes
- [ ] `just test-c` passes
- [ ] Zephyr C examples build

**Files:** `packages/core/nros-c/include/nros/*.h`,
`packages/core/nros-c/src/*.rs`, `CMakeLists.txt`,
`zephyr/CMakeLists.txt`, `zephyr/cmake/*.cmake`,
`packages/codegen/packages/nano-ros-codegen-c/`,
all C example `main.c` files, CLAUDE.md, various docs

---

### 49.2 — nros-c Executor Migration

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

- [ ] Add `nros-node` as explicit dependency in `nros-c/Cargo.toml` (or
  access via `nros` unified crate)
- [ ] Define concrete `CExecutor` type alias with build-time constants
- [ ] Rewrite `nros_executor_init()` to create `Executor::from_session_ptr()`
- [ ] Refactor `nros_subscription_init()` to store metadata only (no RMW
  subscriber creation)
- [ ] Rewrite `nros_executor_add_subscription()` to call
  `executor.add_subscription_raw()` — creates subscriber + registers callback
- [ ] Refactor `nros_service_init()` to store metadata only
- [ ] Rewrite `nros_executor_add_service()` to call
  `executor.add_service_raw()` — creates service + registers callback
- [ ] Rewrite `nros_executor_add_timer()` to delegate to `add_timer()`
- [ ] Rewrite `nros_executor_add_guard_condition()` to delegate
- [ ] Rewrite `nros_executor_set_trigger()` — wrap C fn ptr as
  `Trigger::Predicate`
- [ ] Rewrite `nros_executor_set_semantics()` to delegate
- [ ] Rewrite `nros_executor_spin_some()` to call `executor.spin_once()`
- [ ] Rewrite `nros_executor_spin()` as loop over `spin_once()`
- [ ] Rewrite `nros_executor_spin_period()` to delegate
- [ ] Rewrite `nros_executor_spin_one_period()` to delegate
- [ ] Keep built-in trigger functions as C-exported wrappers
- [ ] Remove self-implemented dispatch logic, handle arrays, LET buffers

**Files:** `nros-c/src/executor.rs`, `nros-c/src/subscription.rs`,
`nros-c/src/service.rs`, `nros-c/Cargo.toml`

---

### 49.3 — nros-c Timer and Guard Condition Migration

**Timer:** Replace `nros_timer_t._internal` with nros-node's Timer type.
Init, cancel, reset, call, and is_ready all delegate to Rust.

| C API function (post-rename) | Delegates to       |
|------------------------------|--------------------|
| `nros_timer_init()`          | `Timer::new()`     |
| `nros_timer_cancel()`        | `timer.cancel()`   |
| `nros_timer_reset()`         | `timer.reset()`    |
| `nros_timer_call()`          | `timer.call()`     |
| `nros_timer_is_ready()`      | `timer.is_ready()` |
| `nros_timer_get_period()`    | `timer.period()`   |

**Expected reduction:** ~348 → ~150 lines.

**Guard condition:** Replace `nros_guard_condition_t._internal` with
nros-node's `GuardCondition`. Trigger, clear, and callback all delegate.

| C API function (post-rename)         | Delegates to            |
|--------------------------------------|-------------------------|
| `nros_guard_condition_init()`        | `GuardCondition::new()` |
| `nros_guard_condition_trigger()`     | `guard.trigger()`       |
| `nros_guard_condition_clear()`       | `guard.clear()`         |
| `nros_guard_condition_is_triggered()`| `guard.is_triggered()`  |

**Expected reduction:** ~450 → ~150 lines.

**Tasks:**

- [ ] Rewrite `nros_timer_init()` to create nros-node Timer
- [ ] Delegate `cancel()`, `reset()`, `call()`, `is_ready()`, `get_period()`
- [ ] Rewrite `nros_guard_condition_init()` to create nros-node
  GuardCondition
- [ ] Delegate `trigger()`, `clear()`, `is_triggered()`
- [ ] Remove self-implemented state machines from both files

**Files:** `nros-c/src/timer.rs`, `nros-c/src/guard_condition.rs`

---

### 49.4 — nros-c Action Migration

Rewrite action server and client to wrap nros-node's action types using
raw-bytes variants. Goal state machine, concurrent goal tracking, and
feedback publishing all delegate to nros-node.

**Prerequisites:** Raw-bytes action entry types in nros-node (similar to 49.1
but for action sub-services: goal request, cancel request, result request,
feedback publish, status publish).

**Delegation approach:**

Like subscriptions (49.2), action init functions store metadata only. The
executor registration (`nros_executor_add_action_server()`) calls
`executor.add_action_server_raw()`, which creates all sub-services and
registers callbacks.

**Delegation table:**

| C API function (post-rename)       | Delegates to                        |
|------------------------------------|-------------------------------------|
| `nros_action_server_init()`        | stores metadata only                |
| `nros_executor_add_action_server()`| `executor.add_action_server_raw()` |
| `nros_action_send_result()`        | `action_server.send_result()`       |
| `nros_action_publish_feedback()`   | `action_server.publish_feedback()`  |
| `nros_action_client_init()`        | stores metadata only                |
| `nros_executor_add_action_client()`| `executor.add_action_client_raw()` |
| `nros_action_send_goal()`          | `action_client.send_goal()`         |
| `nros_action_get_result()`         | `action_client.get_result()`        |

**Expected reduction:** ~1,086 → ~400 lines.

**Tasks:**

- [ ] Add raw-bytes action server entry type to nros-node arena
- [ ] Add raw-bytes action client entry type to nros-node arena
- [ ] Implement raw-bytes dispatch for action sub-services
- [ ] Refactor `nros_action_server_init()` to store metadata only
- [ ] Rewrite `nros_executor_add_action_server()` to delegate
- [ ] Refactor `nros_action_client_init()` to store metadata only
- [ ] Rewrite `nros_executor_add_action_client()` to delegate
- [ ] Delegate goal state transitions to nros-node
- [ ] Delegate feedback publishing to nros-node
- [ ] Delegate result handling to nros-node
- [ ] Remove self-implemented goal state machine and UUID tracking

**Files:** `nros-c/src/action.rs`, `nros-node/src/executor/action.rs`

---

### 49.5 — Tests and Verification

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
marshaling, raw CDR bytes, and metadata storage:

| Module            | Lines | Reason                                                            |
|-------------------|------:|-------------------------------------------------------------------|
| `publisher.rs`    |   549 | Init creates `RmwPublisher` directly (C has no generics)          |
| `subscription.rs` |   465 | Simplified to metadata storage; RMW creation moves to executor    |
| `service.rs`      |   838 | Simplified to metadata storage; RMW creation moves to executor    |
| `cdr.rs`          | 1,174 | Raw CDR serialization for C struct types                          |
| `parameter.rs`    | 1,222 | Parameter server C bindings                                       |
| `lifecycle.rs`    |   728 | Lifecycle state machine C bindings                                |
| `node.rs`         |   349 | Metadata container (`#[repr(C)]`)                                 |
| `clock.rs`        |   315 | Clock C bindings                                                  |
| `support.rs`      |   283 | Session/support init                                              |
| `platform.rs`     |   183 | Platform abstraction C bindings                                   |
| `qos.rs`          |   122 | QoS profile C bindings                                            |
| `lib.rs`          |    85 | Module declarations                                               |
| `error.rs`        |    50 | Error code definitions                                            |
| `constants.rs`    |    28 | Build-time constants                                              |
| `config.rs`       |     6 | Config re-exports                                                 |

Note: `subscription.rs` and `service.rs` are simplified during 49.2 — their
init functions change from "create RMW handle" to "store metadata". The RMW
handle creation moves into the executor registration path, which delegates to
nros-node.

---

## Files to Create/Modify

**49.1 (C API rename):**

| File                                | Changes                                    |
|-------------------------------------|--------------------------------------------|
| `nros-c/include/nros/*.h` (20 files)| Rename `nano_ros_*` → `nros_*` in all decls|
| `nros-c/src/*.rs` (19 files)        | Rename `nano_ros_*` → `nros_*` in FFI fns  |
| `CMakeLists.txt`                    | `NANO_ROS_RMW` → `NROS_RMW`               |
| `cmake/*.cmake`                     | `NanoRos` → `Nros` in targets and configs  |
| `zephyr/CMakeLists.txt`             | Update target names                        |
| `zephyr/cmake/*.cmake`              | Update function/target names               |
| `nano-ros-codegen-c/`               | Emit `nros_*` names in generated code      |
| C example `main.c` files (14)       | Update all API calls                       |
| `CLAUDE.md`                         | Update naming convention section            |
| Various docs (~10 .md files)        | Update references                          |

**49.2–49.4 (nros-c delegation):**

| File                               | Changes                                                     |
|------------------------------------|-------------------------------------------------------------|
| `nros-c/src/executor.rs`          | Rewrite to delegate to nros-node `Executor`                  |
| `nros-c/src/subscription.rs`      | Simplify to metadata storage (RMW creation moves to executor)|
| `nros-c/src/service.rs`           | Simplify to metadata storage (RMW creation moves to executor)|
| `nros-c/src/timer.rs`             | Rewrite to delegate to nros-node `Timer`                     |
| `nros-c/src/guard_condition.rs`   | Rewrite to delegate to nros-node `GuardCondition`            |
| `nros-c/src/action.rs`            | Rewrite to delegate to nros-node action types                |
| `nros-c/Cargo.toml`               | Ensure `nros-node` dependency                                |

---

## Verification

1. `just quality` — full format + clippy + nextest + miri + QEMU
2. `just test-c` — all C API tests pass unchanged
3. Zephyr C examples build and run
4. Native C examples build and run
5. New unit tests for raw-bytes dispatch, guard conditions, LET semantics
6. Kani bounded model checking on new types
7. Line count audit: migrated modules reduced by ~60%
