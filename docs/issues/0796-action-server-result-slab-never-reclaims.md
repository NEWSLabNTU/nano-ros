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

## Problem 1 — the result slab is never reclaimed

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

`RESULT_BUF` defaults to `DEFAULT_RX_BUF_SIZE`, so the ceiling is a few
kilobytes of accumulated results. A server that completes goals in a loop stops
working after a bounded number of them and keeps reporting success.

rcl handles this with a per-goal `result_timeout` plus
`rcl_action_expire_goals()`, which reclaims the storage of goals whose result has
been collected or has aged out. We have **neither the timeout nor the
reclamation** — recorded in the action stage as a `gap` on
`c:action_expire_goals`.

## Problem 2 — the C++ callback tier cannot abort or cancel a goal

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
