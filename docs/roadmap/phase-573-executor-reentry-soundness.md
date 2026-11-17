# Phase 573 — a callback that calls back into its executor is sound

**Status (2026-11-17). OPEN: W1–W4 are open.** Fixes
[issue 2675](../issues/2675-callback-reentry-aliases-executor.md) by amending
[RFC-0108](../design/0108-executor-preemption-levels.md) ("Re-entry from a
callback"). Ahead of [phase-572](phase-572-rust-component-graph-query.md),
whose W2 waits on it. Owner: nros 133.

## Why

A C or C++ callback may call back into the executor it runs on: timer
cancel/reset, releases, parameter calls, graph queries. That is rclc and
rclcpp behaviour users rely on, and the recursive executor lock exists to
allow it. But on the owned pass, Rust holds a live `&mut` to the executor
across the callback in two places, and each re-entering entry derives a
second one. That is undefined behaviour under Rust's aliasing rules.

- **The entry.** `spin_once(&mut self)` is a protected `&mut` for the whole
  pass. The C/C++ spin entries (`rclc_executor_spin*`, `nros_cpp_spin*`)
  reach it through the `DerefMut` of their locked reference.
- **The core.** `Executor::pass` re-derives its `s: &mut Self` from `this`
  after a callback only when a callback WINDOW opened, which is a shared or
  band pass whose lock has a thread id. In an owned pass, or a shared pass
  with no thread-id lock, `s` stays live across `try_process` and is used
  after it.

The windowed shared and band passes are already right. They are what the
FreeRTOS multi-level runner and the RTIC SRP backend (142's W10) use.

The ruling (2026-11-17) is to keep the behaviour and make it sound. Refusing
re-entry from callbacks was rejected: it would take timer cancel, releases,
parameter calls and graph queries out of C and C++ callbacks.

## Work items

### W1 — The design, as an RFC-0108 amendment

- **Status:** OPEN

Who holds what across a callback, written down:
- the dispatch loop holds no reference into the executor across a callback,
  and re-derives from `this` after EVERY callback in every mode, windowed or
  not;
- every spin entry reaches the pass through a raw pointer;
- one helper derives the executor for every FFI entry, so no entry hand-rolls
  the derivation.

### W2 — The dispatch core is sound

- **Status:** OPEN

`pass` re-derives after every callback and every tick. The owned entry is
`unsafe fn spin_once_raw(this: *mut Self, ..)`, which `spin_once(&mut self)`
and the FFI spin entries call. The FFI spin entries pass the locked
reference's pointer and never create a `&mut` that outlives the call.

Acceptance: a pure-Rust harness re-enters the executor from a callback once
per entry kind (timer cancel and reset, a release, a parameter call, a graph
query, a service-client check). It reaches the executor through a raw pointer,
as the FFI does. Run under Miri with Tree Borrows, it passes on the new core
and FAILS on the old one, which is the negative control. The dispatch core is
testable with no FFI and no RMW backend.

### W3 — One derive helper, and the class sweep

- **Status:** OPEN

`get_executor`, `get_executor_from_ptr`, `cpp_ctx_checked`, `cpp_ctx_locked`
and the `CLocked` / `Locked` lookups go through one helper, measured at
2026-11-17 as 224 call sites in nros-c and nros-cpp. A sweep lists every
executor-deriving FFI entry and proves each one goes through the helper or
returns `RCLC_RET_REENTRANT` / `Reentrant` from a callback.

Acceptance: the sweep is a gate, so a new entry that derives the executor
another way fails it.

### W4 — The multi-level and RTIC case

- **Status:** OPEN

A callback that re-enters, for example with a timer reset, while a
higher-priority level preempts it. The shared and band passes, and the RTIC
SRP backend (whose ceiling is not held across a callback), keep W2's
property. 142's W10 adds the same shape on its backend.

Acceptance: the W2 harness runs that case through `spin_band_once` with two
bands.
