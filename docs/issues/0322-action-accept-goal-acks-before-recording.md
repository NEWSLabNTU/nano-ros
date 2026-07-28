---
id: 322
title: "accept_goal replies accepted=true BEFORE recording the goal — the 5th concurrent goal is acknowledged and then dropped, client waits forever"
status: resolved
type: bug
severity: high
area: core
related: [issue-0269]
---

## Finding (audit 2026-07-28, P1 — lead-verified)

`packages/core/nros-node/src/executor/action_core.rs:280-299`:

```rust
pub fn accept_goal(&mut self, goal_id: GoalId, seq: i64) -> Result<(), NodeError> {
    // … serialize accepted=true + stamp …
    self.send_goal_server
        .send_reply(seq, &self.cancel_buffer[..reply_len])
        .map_err(|_| NodeError::ServiceReplyFailed)?;

    let _ = self.active_goals.push(RawActiveGoal {   // <-- after the reply, result discarded
        goal_id,
        status: GoalStatus::Accepted,
    });
    let _ = self.publish_status_array();
    Ok(())
}
```

- `active_goals` is `heapless::Vec<RawActiveGoal, MAX_GOALS>` with
  **`MAX_GOALS` defaulting to 4** (`action_core.rs:164,171`).
- No caller pre-checks capacity — verified: the only call sites are
  `nros-c/src/action/server.rs:1081` and `nros-cpp/src/action.rs:2044`, both of
  which just forward.

So with 4 goals already active, a 5th `send_goal` request gets
`accepted=true` on the wire and the server keeps **no record of it**: the goal
never executes, no feedback, no result, no terminal status. An rclcpp/rclpy
client that received `accepted=true` then waits on the result future forever.

This is the same masking pattern as issue 0269 (a hardcoded 4-slot pool whose
overflow was swallowed and surfaced as an unrelated symptom) — a bounded table
plus a discarded `push` result.

## Fix

1. Push into `active_goals` **before** `send_reply`; on a full table call
   `reject_goal(seq)` so the client gets a truthful `accepted=false`.
2. Propagate `publish_status_array()` failure instead of `let _ =`.
3. Consider whether `MAX_GOALS = 4` should be a named, documented capacity knob
   (Kconfig / board metadata) rather than a default const — an action server
   that silently caps at 4 concurrent goals is a surprising contract, and the
   number appears nowhere in the user-facing docs.

## Resolved (2026-07-28)

### Item 1 — record before acknowledging (done)

`accept_goal` now decides capacity **before** anything reaches the wire:

1. serialize the reply (a failure here leaves no trace — nothing yet touched
   the table);
2. `active_goals.push(...)`; on `Err` return `self.reject_goal(seq)`, so a full
   table produces an honest `accepted=false` — a contract rclcpp/rclpy clients
   already handle;
3. `send_reply`; on failure **`active_goals.pop()`** and return
   `ServiceReplyFailed`.

The rollback in (3) was not in the issue's prescription but is required by it:
recording first means a failed reply would otherwise leak a slot and
permanently lower the effective capacity for every later goal. `pop()` is
correct because `&mut self` guarantees nothing touched the table between the
push and the failure.

### Item 2 — status-array failure deliberately NOT propagated

The issue asks for `publish_status_array()?` instead of `let _ =`. Implementing
that literally makes the fix worse, so it was not done, and the reasoning is
recorded in the code:

By the time the status array is published, `send_reply` has succeeded — the
acceptance is on the wire and irreversible. Both callers
(`nros-c/src/action/server.rs:1081`, `nros-cpp/src/action.rs:2044`) collapse
`Err` into a single generic error code, so propagating here reports "accept
failed" for a goal that **is** accepted and running, inviting the caller to
reject or retry it. The client already holds `accepted=true` and will still
receive its result; a missed status sample is degraded, not broken.

The `let _ =` is kept but named (`let _status = …`) and commented, so it reads
as a decision rather than an oversight. Propagating it properly needs a return
type that can say "accepted, but status publish failed" — a wider API change
than this bug.

### Item 3 — `MAX_GOALS` as a documented knob: not done

Still worth doing, still surprising, and now at least *safe*: exceeding it is a
clean rejection instead of a silent drop. It is a const generic on
`ActionServerCore` with a default of 4, so exposing it as a Kconfig/board knob
touches the C and C++ opaque-size mirrors (`nros-c/src/opaque_sizes.rs:56`
already hardcodes the assumption). Left open as follow-up.

## Coverage gap (honest)

**No test exercises the 5th goal**, and none can today. `ActionServerCore`
needs live RMW server handles, so there is no unit-test seam; and
`tests/actions.rs:163` states the e2e limitation directly:

> `- Multiple concurrent goals: Requires multi-goal support in client`

Closing this properly needs a client fixture that keeps N goals in flight —
a build-step fixture plus example, not a test-time change. Until then the fix
rests on the ordering being self-evident rather than on a regression test,
which is exactly the weakness that let the original land.

What was verified: `cargo build`/`clippy -p nros-node` clean, and the action
e2e suite green after building its fixtures — `test_action_server_starts`,
`test_action_client_starts`, `test_action_server_client_communication`,
`test_action_binaries_exist` all PASS, so the accept → feedback → result
round-trip still works.
