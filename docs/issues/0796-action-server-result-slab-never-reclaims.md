---
id: 796
title: "Action server: the result slab only ever grows, so a long-running server
  silently stops delivering results; and the C++ callback tier reports every
  goal as SUCCEEDED"
status: open
type: bug
area: core, api
related: [rfc-0069, phase-379, phase-237]
---

## Problem 1 — the result slab is never reclaimed (FIXED 2026-08-25)

`ActionServerCore::complete_goal_raw`
(`packages/core/nros-node/src/executor/action_core.rs:449-464`) appends each
completed goal's result CDR to a fixed slab:

```rust
let offset = self.result_slab_used;
let end = offset + result_cdr.len();
let stored = if end <= RESULT_BUF {
    self.result_slab[offset..end].copy_from_slice(result_cdr);
    self.result_slab_used = end;
    ...
    true
} else {
    false
};
```

`result_slab_used` is set to `0` once at construction (line 252) and thereafter
only ever assigned `end`. **Nothing resets it** — there is no reclamation path
anywhere in the file.

So once the slab fills, `stored` is `false` for every subsequent goal and:

* the result is dropped,
* the `pending_get_results` flush below is skipped, so any client already waiting
  on `~/_action/get_result` waits forever,
* and `complete_goal_raw` returns `()` — the server's own callback is told
  nothing.

`RESULT_BUF` defaults to `DEFAULT_RX_BUF_SIZE` (1024), so the ceiling is about a
kilobyte of accumulated results. A server that completes goals in a loop stops
working after a bounded number of them and keeps reporting success.

There were in fact **two** leaks with one cause — nothing was ever evicted.
`result_slab_used` only moved forward, and `completed_results`
(`heapless::Vec<_, MAX_GOALS>`, default 4) was only ever `push`ed, with the push
discarded by `let _ =`. Whichever filled first stopped the server. The typed
mirror `ActionServer::completed_goals` was push-only too, and read by nobody.

rcl handles this with a per-goal `result_timeout` plus
`rcl_action_expire_goals()`, which reclaims the storage of goals whose result has
been collected or has aged out. We had **neither the timeout nor the
reclamation** — recorded in the action stage as a `gap` on
`c:action_expire_goals`.

### Fixed — compaction, plus demand-driven delivered-first eviction

**Storage shape: compaction, not partitioning.** The slab stays one bump region;
`completed_results` is held in completion order, which is also increasing-offset
order, and on every reclamation `compact_result_slab()` walks the survivors in
that order, memmoves each down to the first free byte and rewrites its offset.
Bounded and infrequent: at most `MAX_GOALS` (4) moves totalling at most
`RESULT_BUF` bytes, only when a completion needs room.

The alternative — partitioning the slab into `MAX_GOALS` fixed slots, which makes
eviction O(1) and bounds the worst case per goal rather than in total — was
rejected on its cost: with the shipped defaults it would cut the largest storable
result from 1024 bytes to 256, and that cut is **permanent**, not transient. A
result between 257 and 1024 bytes works today and would stop working forever,
failing even against an empty slab. Compaction keeps "any single result up to
`RESULT_BUF`" intact and pays for it with a few-KB memmove on eviction. Every
entry that could hold that memmove is capped by `MAX_GOALS`, so the price is
bounded where the capability loss would not have been.

**Eviction policy.** rcl keeps a result until it is collected AND a
`result_timeout` elapses. We have no clock in the core (it is `no_std` and no
time source is threaded through it), so reclamation is on demand and
priority-ordered instead of timed:

1. the oldest result already **delivered** to a client, else
2. the oldest result overall.

Rule 1 means a fetched result never pins storage — the case rcl's timeout really
serves. Rule 2 means a client that asks for nothing cannot wedge the server: its
stale result is displaced by newer ones. A goal evicted under rule 2 whose client
asks later is answered `GoalStatus::Unknown` with the default result — degraded,
but an answer, where the old code left the requester hanging forever.
`expire_completed_results()` is the eager analogue of `rcl_action_expire_goals()`
for a server that would rather return the memory before an idle period; calling
it is optional.

**The silence is gone.** `ActionServerCore::complete_goal_raw` returns
`Result<(), NodeError>`, and so do `ActionServerRawHandle::complete_goal_raw`,
`ActionServerHandle::complete_goal` and `ActionServer::complete_goal`. A full
slab is no longer a failure at all — it is reclaimed — so the one remaining error
is a result larger than `RESULT_BUF`, which names the knob to raise. The C and
C++ entry points already returned status codes, so they now propagate it with no
header change (`check-cbindgen-headers` green without a regen).

Two more silent losses fell out of the same read:

* the `pending_get_results` flush sat inside `if stored`, so a client that had
  already sent `get_result` was **never answered** when the store failed. The
  waiter is now answered from the caller's own bytes — only the *retention*
  failed, the bytes are right there.
* `ActionServer::complete_goal` treated a serialization failure as a zero-length
  result and stored it as if it were real. It now reports
  `NodeError::Serialization`.

Regression tests live in `packages/core/nros-node/src/executor/tests.rs`
(`action_results_keep_being_delivered_past_the_slab_capacity` and four siblings).
Mutation-checked: with reclamation reverted and everything else kept, three of
them fail at goal 4 — exactly where a 128-byte slab of 40-byte results fills.

## Problem 2 — the C++ callback tier cannot abort or cancel a goal (FIXED 2026-08-25)

`packages/api/nros-cpp/src/action.rs:445`:

```rust
h.complete_goal_raw(
    &mut ctx.executor,
    &id,
    nros::GoalStatus::Succeeded,   // hardcoded
    result_fields,
);
```

The public `nros::ActionServer<A>::complete_goal(goal_id, result)` takes no
status, and the shim supplies `Succeeded` unconditionally. **A C++
callback-tier server that aborts a goal reports it to the client as
succeeded.**

Every other surface takes a status: C has `nros_action_abort` and
`nros_action_canceled`, the C++ *polling* tier takes one, and both Rust servers
take one. This is the C++ callback tier alone.

**Fixed.** The shim takes an `int32_t status` and validates it through
`GoalStatus::from_i8`; `ActionServer::complete_goal(goal_id, status, result)`
matches `PollingActionServer::complete_goal` and the C API's
`nros_action_server_complete_goal_raw`, so the three surfaces now agree on the
parameter order. A two-argument overload forwards `Succeeded`, so existing calls
keep compiling.

Worth recording how the mirrors were found: the declaration existed in **three**
places — `nros_cpp_ffi.h`, `nros-c/component.h` and `action_server.hpp` — and
editing two of them compiled fine. `just check-c`'s cross-include TU (the gate
CLAUDE.md describes for exactly this drift class) caught the third with
`conflicting types`. Six example call sites used the raw FFI directly and were
updated. `just check-c` and `just check-cpp` are green.

**Both problems are now fixed.** What remains open in this issue is the
`Related` list below — the C++ callback tier's missing accepted-callback and
client-side cancel, and the `CancelResponse` naming collision.

## Related, from the same stage

* **No accepted-callback in C++.** C takes `nros_accepted_callback_t` at
  `nros_action_server_init`, Rust takes one in
  `create_action_server_with_callbacks(goal, cancel, accepted)`, and
  `nros::ActionServer<A>` has only `set_goal_callback`/`set_cancel_callback`. A
  C++ user who returns ACCEPT_AND_DEFER is never told the goal was accepted.
* **Client-side cancel is missing from the C++ callback tier only.**
* **`CancelResponse` names two different things.** In C/C++ it is the per-goal
  Reject/Accept decision; `nros_core::CancelResponse` is the
  `action_msgs/srv/CancelGoal` return code — and `CallbackCtx::set_cancel_response`
  takes the RPC-level enum to express a per-goal decision. C is the only language
  that names them apart (`nros_cancel_response_t` vs
  `nros_cancel_return_code_t`).
* **`GoalResponse` and `CancelResponse` correlate as `same` against
  rclcpp_action and are not drop-in**: our discriminants are 0-based where
  rclcpp_action's are 1-based, and our enumerators are `Reject` where theirs are
  `REJECT`. Not wire values, so not an interop bug — but the correlator cannot
  see it, because enumerator comparison is a feature it does not have.

## Evidence

`packages/core/nros-node/src/executor/action_core.rs:218,252,449-464`;
`packages/api/nros-cpp/src/action.rs:440-450`;
`scripts/api-parity.py --topic action` and
`docs/reference/api-parity-ledger/action.json`.

## Direction

Not decided here. Problem 2 is a small, contained fix — thread the status
through `complete_goal` as every other surface already does. Problem 1 is a
design question: a bump allocator with no free needs either a reclamation pass
(rcl's shape: a result timeout plus an expiry sweep) or a different storage
shape (per-goal slots sized at declaration, which suits a static entity table
better and bounds the worst case instead of the total). Whichever is chosen, the
overflow path must stop being silent — `complete_goal_raw` returning `()` is how
this stayed invisible.
