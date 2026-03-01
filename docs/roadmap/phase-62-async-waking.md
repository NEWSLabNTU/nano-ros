# Phase 62 — Event-Driven Async Waking

**Goal**: Replace busy-polling in `Promise`, `FeedbackStream`, and `spin_async()` with
proper event-driven waking via `AtomicWaker`, enabling WFI sleep between events on all
async executors.

**Status**: Not Started

**Priority**: Medium

**Depends on**: None (benefits tokio, Embassy, RTIC equally)

## Overview

All nano-ros async types (`Promise`, `FeedbackStream`, `GoalFeedbackStream`) and
`spin_async()` currently busy-poll by calling `cx.waker().wake_by_ref()` on `Pending`.
This immediately re-schedules the task, burning CPU cycles and preventing WFI sleep.
The CPU utilization problem affects every async executor: tokio wastes thread time,
Embassy/RTIC waste battery by never entering WFI.

The root cause: **no waker storage exists anywhere in nano-ros**. When data arrives via
C callbacks, there is no way to notify the Rust `Future::poll()` caller.

The fix: add `AtomicWaker` per handle type, woken from transport callbacks when data
arrives. This is a default (non-optional) dependency — busy-polling should not be the
default behavior.

## Design

### Dependency: `atomic-waker`

```toml
# In nros-node/Cargo.toml:
atomic-waker = { version = "1.1", default-features = false, features = ["portable-atomic"] }
```

The `portable-atomic` feature is **required** to support all nano-ros targets.
`nros-node` already depends on `portable-atomic = { version = "1", default-features = false }`.

**Platform compatibility**:

| Target             | Architecture       | Native CAS | `portable-atomic` needed?                                |
|--------------------|--------------------|------------|----------------------------------------------------------|
| x86-64             | x86-64             | Yes        | No (zero overhead — delegates to `core::sync`)           |
| thumbv7m-none-eabi | Cortex-M3 (ARMv7)  | Yes        | No (zero overhead)                                       |
| thumbv7em-none-eabi| Cortex-M4F (ARMv7E)| Yes        | No (zero overhead)                                       |
| riscv32imc (ESP32) | RISC-V (no A ext)  | **No**     | Yes — board crate enables `unsafe-assume-single-core`    |
| thumbv6m (future)  | Cortex-M0 (ARMv6)  | **No**     | Yes — board crate would enable `unsafe-assume-single-core`|
| Xtensa (ESP32)     | Xtensa LX6/7       | Yes        | No (zero overhead)                                       |

Board crates for CAS-less targets (ESP32-C3) already enable `unsafe-assume-single-core`
on `portable-atomic`, which Cargo unifies across the dependency tree. No new burden on
any existing target.

**Do NOT use `atomic-waker` without the `portable-atomic` feature** — it would try
`core::sync::atomic::AtomicUsize` which doesn't exist on riscv32imc, breaking ESP32-C3.

**Alternative considered**: `futures-core`'s built-in `AtomicWaker`. Rejected because
it pulls in the full `futures-core` crate and uses `portable-atomic` with
`features = ["require-cas"]` (compile error on CAS-less targets instead of helpful
delegation). The standalone `atomic-waker` crate is ~200 lines with no transitive deps.

### Current Busy-Poll Locations

| Location                              | Line   | Pattern                                     |
|---------------------------------------|--------|---------------------------------------------|
| `Promise::poll()`                     | 300    | `cx.waker().wake_by_ref()` on `Pending`     |
| `FeedbackStream::poll_recv()`         | 737    | `cx.waker().wake_by_ref()` on `Pending`     |
| `GoalFeedbackStream::poll_recv()`     | 860    | `cx.waker().wake_by_ref()` on `Pending`     |
| `GoalFeedbackStream::poll_next()`     | 880    | `cx.waker().wake_by_ref()` on `Pending`     |
| `spin_async()`                        | 1210   | `cx.waker().wake_by_ref()` on `Pending`     |

All in `packages/core/nros-node/src/executor/handles.rs` and `spin.rs`.

### Zenoh-pico Subscriber Path (no C changes)

The C shim already calls a Rust function (`subscriber_notify_callback`) when data
arrives, which sets `SubscriberBuffer.has_data` atomically. Add an `AtomicWaker` to
`SubscriberBuffer`; register in `poll()`, wake from callback.

```
C sample_handler() → Rust subscriber_notify_callback()
                     → buffer.has_data.store(true)
                     → buffer.waker.wake()        ← NEW
```

This also enables a new `Subscription` Future/Stream implementation (currently
subscriptions have no async API — only `try_recv()`).

### Zenoh-pico Service Client Path (C shim change needed)

Replies arrive in C via `pending_get_reply_handler()`, which sets
`pending_get_slot_t.ctx.received = true` (a plain `bool`, not atomic). There is no
Rust callback in this path. Fix: add a waker callback hook to zpico.c per pending-get
slot.

**C side** (new in zpico.c):
```c
typedef void (*zpico_waker_fn)(int32_t slot);
static zpico_waker_fn g_reply_waker = NULL;

void zpico_set_reply_waker(zpico_waker_fn fn);

// In pending_get_reply_handler():
if (g_reply_waker) g_reply_waker(slot_index);
```

**Rust side**:
```rust
static REPLY_WAKERS: [AtomicWaker; ZPICO_MAX_PENDING_GETS] = ...;

extern "C" fn reply_waker_callback(slot: i32) {
    REPLY_WAKERS[slot as usize].wake();
}
```

Register `reply_waker_callback` at session init via `zpico_set_reply_waker()`.

### XRCE-DDS Subscriber Path (no C changes)

Add `AtomicWaker` field to `SubscriberSlot`. In `topic_callback()`: call
`waker.wake()` after `has_data.store(true)`. Same registration pattern in `poll()`.

### XRCE-DDS Service Client Path (no C changes — callbacks are in Rust)

Add `AtomicWaker` field to `ServiceClientSlot`. In `reply_callback()`: call
`waker.wake()` after `has_reply.store(true)`. Simpler than zpico because all XRCE
callbacks are already Rust functions.

### Updated Async Availability After This Phase

| Type                        | `.await`                        | Notes                         |
|-----------------------------|---------------------------------|-------------------------------|
| `Promise` (service reply)   | **Yes** — event-driven          | Waker fires on reply arrival  |
| `Promise` (goal acceptance) | **Yes** — event-driven          | Same                          |
| `Promise` (action result)   | **Yes** — event-driven          | Same                          |
| `FeedbackStream`            | **Yes** — event-driven          | Waker fires on feedback       |
| `Subscription`              | **Yes** — NEW Future/Stream     | Waker fires on data arrival   |
| `ServiceServer`             | **No** — still poll-based       | Must use `Mono::delay().await`|

## Work Items

- [ ] 62.1 — Add AtomicWaker to zpico subscriber path
- [ ] 62.2 — Add AtomicWaker to zpico service client path (C shim change)
- [ ] 62.3 — Add AtomicWaker to XRCE subscriber path
- [ ] 62.4 — Add AtomicWaker to XRCE service client path
- [ ] 62.5 — Remove busy-poll from handles.rs and spin.rs

### 62.1 — Zpico Subscriber AtomicWaker

Add `AtomicWaker` to `SubscriberBuffer`. Wake from `subscriber_notify_callback()`.
Implement `Future` and optionally `Stream` for `Subscription`.

**Status**: Not Started

**Files**:
- `packages/zpico/nros-rmw-zenoh/src/shim/subscriber.rs` — AtomicWaker in buffer
- `packages/core/nros-node/src/executor/handles.rs` — `Subscription` Future/Stream impl

### 62.2 — Zpico Service Client AtomicWaker

Add `zpico_waker_fn` callback type and `zpico_set_reply_waker()` to zpico.c. Add
static `REPLY_WAKERS` array on Rust side. Register callback at session init.

**Status**: Not Started

**Files**:
- `packages/zpico/zpico-sys/c/zpico/zpico.c` — `zpico_set_reply_waker()`, callback hook
- `packages/zpico/zpico-sys/c/zpico/zpico.h` — new function declaration
- `packages/zpico/nros-rmw-zenoh/src/shim/service.rs` — static REPLY_WAKERS, registration

### 62.3 — XRCE Subscriber AtomicWaker

Add `AtomicWaker` to `SubscriberSlot`. Wake from `topic_callback()`.

**Status**: Not Started

**Files**:
- `packages/xrce/nros-rmw-xrce/src/lib.rs` — AtomicWaker in SubscriberSlot, wake in callback

### 62.4 — XRCE Service Client AtomicWaker

Add `AtomicWaker` to `ServiceClientSlot`. Wake from `reply_callback()`. No C changes
needed — all XRCE callbacks are already Rust functions.

**Status**: Not Started

**Files**:
- `packages/xrce/nros-rmw-xrce/src/lib.rs` — AtomicWaker in ServiceClientSlot, wake in callback

### 62.5 — Remove Busy-Poll from Core

Remove `wake_by_ref()` calls from all `Future::poll()` and `Stream::poll_next()`
implementations. Update `spin_async()` to use proper waking. Add `atomic-waker`
dependency to `nros-node`.

**Status**: Not Started

**Files**:
- `packages/core/nros-node/src/executor/handles.rs` — all `poll()` impls
- `packages/core/nros-node/src/executor/spin.rs` — `spin_async()`
- `packages/core/nros-node/Cargo.toml` — add `atomic-waker` dependency

## Acceptance Criteria

- [ ] `Promise.await` does not busy-poll (no `wake_by_ref()` calls remain)
- [ ] `FeedbackStream.recv().await` does not busy-poll
- [ ] `spin_async()` does not busy-poll
- [ ] `Subscription` has a Future or Stream implementation (new capability)
- [ ] Works on all existing targets (thumbv7m, thumbv7em, riscv32imc, x86-64)
- [ ] Embassy executor enters WFI between events (verified on QEMU or real hardware)
- [ ] No regression for synchronous (`try_recv()` / `spin_once()`) usage
- [ ] `just quality` passes

## Notes

- **Not RTIC-specific**: This phase benefits Embassy (WFI sleep), tokio (reduced
  thread spinning), and any future async executor equally
- **ServiceServer remains poll-based**: There is no natural "request arrived" callback
  for queryables in the zpico path — the queryable handler writes to a stored-query
  slot but doesn't have a per-server waker hook. This could be added in a future phase
- **`atomic-waker` vs `futures-core`**: `atomic-waker` (smol-rs) is preferred over
  `futures_core::task::AtomicWaker` because: (a) standalone ~200 lines vs full
  `futures-core` dependency, (b) uses `portable-atomic` without `require-cas` (avoids
  compile errors on CAS-less targets), (c) already battle-tested in smol ecosystem
- **Default dependency rationale**: Busy-polling wastes power on battery-operated
  embedded devices, which is nano-ros's primary target. Making event-driven waking
  the default aligns with embedded best practices
