# Phase 573 — a callback that calls back into its executor is sound

**Status (2026-11-17). OPEN: W1 and W2 are done; W2b, W3 and W4 are open.** Fixes
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

One pull request per row, each with its failing Miri case committed first.

| Entry kind | Status | Notes |
|---|---|---|
| Raw action server | Open | Its own pull request. |
| Service and action client callbacks | Fixed | Wider than re-entry. `ServiceClientCallback` and `ActionClientCallback` held a pointer made as `&mut (*entry).field as *mut _`, a child of a temporary reference, which the dispatcher's own write to the entry disabled. So the second request through the safe handle was UB even from outside any callback. The fix takes a raw place (`addr_of_mut!`); the guard flag's pointer follows suit. The typed action-client dispatch also re-derives the entry per step, and the service-client info closure holds the entry pointer instead of `&hdr.handle`. Cases: a reply that sends the next request, a goal response that asks for its result, and each handle surviving its reply dispatch. Cost: none (host release, 13,752 B for the ten client dispatch monomorphs, before and after). |
| Service server | Open | `handle`, `req_buffer` and `reply_buffer` are `&mut` arguments of `handle_request(.., cb)`. |
| Subscription drain | Open | `entry` is used again on the next loop iteration. |
| Guard | Open (negative control) | Atomic flag; the entry is not used after the callback. |

### W3 — One derive helper, and the class sweep

- **Status:** OPEN

`get_executor`, `get_executor_from_ptr`, `cpp_ctx_checked`, `cpp_ctx_locked`
and the `CLocked` / `Locked` lookups go through one helper, measured at
2026-11-17 as 224 call sites in nros-c and nros-cpp. A sweep lists every
executor-deriving FFI entry and proves each one goes through the helper or
returns `RCLC_RET_REENTRANT` / `Reentrant` from a callback.

Acceptance: the sweep is a gate, so a new entry that derives the executor
another way fails it.

One known site of W2b's client pattern on a different object belongs here:
`Executor::session_ptr` (`spin.rs`) returns `&mut *self.session as *mut _`, a
pointer derived from a temporary reborrow. A caller that keeps it while the
executor writes the session through its own field holds a disabled tag, the
same defect the client handles had. The sweep covers it: the fix is a raw
place, and a Miri case keeps the pointer across a `spin_once`.

### W4 — The multi-level and RTIC case

- **Status:** OPEN

A callback that re-enters, for example with a timer reset, while a
higher-priority level preempts it. The shared and band passes, and the RTIC
SRP backend (whose ceiling is not held across a callback), keep W2's
property. 142's W10 adds the same shape on its backend.

Acceptance: the W2 harness runs that case through `spin_band_once` with two
bands.
