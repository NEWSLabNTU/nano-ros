---
id: 322
title: "accept_goal replies accepted=true BEFORE recording the goal — the 5th concurrent goal is acknowledged and then dropped, client waits forever"
status: open
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
