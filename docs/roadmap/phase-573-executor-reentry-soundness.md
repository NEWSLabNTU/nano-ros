# Phase 573 — a callback that calls back into its executor is sound

**Status (2026-11-19). OPEN: W1, W2 and W5b are done; W2b, W3, W4, W5c, W5d and W5e are open.** Fixes
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

- **Status:** DONE (2026-11-17, #4150: RFC-0108 D9)

Who holds what across a callback, written down:
- the dispatch loop holds no reference into the executor across a callback,
  and re-derives from `this` after EVERY callback in every mode, windowed or
  not;
- every spin entry reaches the pass through a raw pointer;
- one helper derives the executor for every FFI entry, so no entry hand-rolls
  the derivation.

### W2 — The dispatch core is sound

- **Status:** DONE (2026-11-17, the W2 PR: core, entries, timers; gate `miri-executor-reentry`)

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

As built: the harness is `executor::tests::reentry`. Its cases are a timer
cancelling itself, a timer resetting itself, a timer reading its executor,
and a timer releasing another entry. The first commit shows it failing under
Miri on the old core, at the protected `&mut self` of `spin_once`. Three more
findings followed, each from Miri: the stored `&mut` arena (now `ArenaRef`),
the timer's own entry reference held across its callback (re-derived), and
`pass`'s windowed-only re-derive. The last one's negative control fails at
`s.finish_dispatch(i)`, right after the release case's callback. The Miri gate
runs in 11 s cold. A parameter call, a graph query and a service-client check
read or write executor fields that rule 1 covers; their per-kind cases move to
W2b with the other entry kinds.

### W2b — No entry holds its own reference across its callback

- **Status:** OPEN

RFC-0108 D9 rule 4 for every entry kind other than timers. Each
`*_try_process` in `arena.rs` holds `&mut Entry` across the user callback, and
under Tree Borrows that breaks when the callback writes the same entry's bytes.
Audit every FFI entry that can write a running entry: an action server's
`send_result` or feedback from inside its own goal callback, a guard condition
triggered from its own callback, a service reply, and the subscription ring
loop that holds `ring` across callbacks. Fix each site by re-deriving after
the callback, or by deferring the write the way a windowed timer cancel
already does.

Acceptance: a Miri case per kind in `executor::tests::reentry`, run by
`just check miri-executor-reentry`.

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

### W5b — A C entity's fini releases its executor entry

- **Status:** DONE (2026-11-19)

`rcl_subscription_fini` left its arena entry registered, so a fini without
`rclc_executor_remove_subscription` left the executor deserialising into the
caller's `msg` storage, and dispatching into the finalised struct. The same
held for every C entity the executor registers: timer, service, client,
action server and action client.

Each fini now releases the entry when the entity is still added to a live
executor, through the same `detach_*` core the `rclc_executor_remove_*`
calls use (those are now argument checks over it). From a callback its
executor is dispatching, a fini returns `RCLC_RET_REENTRANT` and changes
nothing, as remove does. A fini after `rclc_executor_fini` has nothing left
to release and succeeds: it reads the executor's state, never its zeroed
`_opaque` (the old subscription fini did read it).

Acceptance:
- `entity_remove_cycles.c` and `action_remove_cycles.c` run the create / add
  / fini loop with NO remove, and it must out-run the handle table and the
  arena with the backend's live count back at its baseline every cycle.
  Before W5b these were the negative controls, which exhausted the executor.
- A fini after the executor's fini succeeds; a timer finalising itself from
  its own callback is refused and stays registered.

### W5c — The C++ lifetime contract is written where it is used

- **Status:** OPEN

`create_wall_timer<C, &C::m>(..., C* self)`, the other `create_timer*`
forms and `create_guard_condition(..., context)` keep `self` / `context`
in the executor entry, so the owner must outlive the registration and must
not move while it is registered. `node.hpp` says so at each of them, with a
debug-build check where one is cheap.

Acceptance: each such declaration states the contract; any debug check has a
test that trips it.

### W5d — A sized C subscription registration refuses an undersized buffer

- **Status:** OPEN (user ruling 2026-11-19: option B)

`rclc_executor_add_subscription[_with_context]` take rclc's exact arguments,
so the executor deserialises into `msg` with no size to check it against.

- The generated message descriptor gains `message_size` (the type's
  `sizeof`), appended.
- `rclc_executor_add_subscription_sized` and
  `rclc_executor_add_subscription_with_context_sized` take `msg_size` and
  refuse `msg_size < message_size` with `RCL_RET_INVALID_ARGUMENT`.
- The rclc-named forms keep their signatures; their docs name the sized form
  as the checked one.
- The generated code asserts `message_size` against the struct `__init_in`
  wires.

Acceptance: the FFI-mirror gate covers the appended field; ledger rows for
the two new entry points; a test refuses an undersized buffer and accepts
an exact one.

### W5e — A lane runs AddressSanitizer

- **Status:** OPEN — issue 2728 (being filed with W5a)

W5a's use-after-free showed only under AddressSanitizer, which no lane runs.
A lane that builds the C run probes and the executor harness with ASan makes
that class visible without a hand-run.
