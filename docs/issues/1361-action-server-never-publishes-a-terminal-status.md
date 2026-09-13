---
id: 1361
title: "the action server never publishes a TERMINAL status: `complete_goal_raw`
  removes the goal from `active_goals` before it publishes, so every client sees
  ACCEPTED and then an empty array"
status: open
type: bug
area: core
severity: high
related: [issue-1341, issue-0902, phase-455]
---

## Symptom, measured

`/status` on a nano-ros action server carries exactly two samples for a goal
that runs to completion, and neither is the terminal one. Measured on
2026-09-13 in the `ros2` distrobox against a live `rmw_zenohd` +
`rmw_zenoh_cpp` 0.1.9, with `examples/native/rust/action-server` and a stock
`ros2 action send_goal /fibonacci example_interfaces/action/Fibonacci
'{order: 5}'`.

The subscriber below attached BEFORE the goal, so it saw every sample there is:

```
===== EVERY /status sample this early subscriber saw =====
status_list:
- goal_info:
    goal_id:
      uuid: [182, 25, 253, 64, 32, 197, 76, 238, 149, 89, 111, 192, 194, 16, 143, 232]
    stamp: {sec: 0, nanosec: 0}
  status: 1
---
status_list: []
---
```

`status: 1` is `STATUS_ACCEPTED`. There is no `status: 4` (`STATUS_SUCCEEDED`)
sample, and the server's own TRACE log agrees — two publishes on the status
publisher, 33 bytes then 8 bytes, where 8 bytes is a CDR header plus a
zero-length sequence:

```
[TRACE] Publishing 33 bytes with attachment: seq=1, ... gid=[b5, d3, ff, 83]
... (six feedback publishes on the other publisher) ...
[TRACE] Publishing  8 bytes with attachment: seq=2, ... gid=[b5, d3, ff, 83]
[INFO] Goal succeeded
```

The goal DID succeed: the stock client printed `Goal finished with status:
SUCCEEDED`, which it learned from the `get_result` reply, not from `/status`.

## Cause

`ActionServerCore::complete_goal_raw`
(`packages/core/nros-node/src/executor/action_core.rs:631`) removes the goal
from `active_goals` as its FIRST action:

```rust
// Remove from active goals
if let Some(pos) = self.active_goals.iter().position(|g| g.goal_id.uuid == goal_id.uuid) {
    self.active_goals.swap_remove(pos);
}
```

and `publish_status_array` (`:1077`) serialises `self.active_goals` and nothing
else. The `let _ = self.publish_status_array();` at the end of the same
function therefore publishes the array WITHOUT the goal — the empty one above.
The terminal status is written nowhere on the wire; `completed_results` keeps it
(`CompletedResultEntry::status`) but no publisher reads that list.

This is backend-independent: it is in `nros-node`, below the RMW seam, so
cyclone and XRCE images have it too.

## Why it matters, and what it is NOT

It is the "goal terminated, client never saw it" shape of issue 0902, one layer
up from where 0902 has been looking. A client that misses the `get_result`
reply — or that only watches `/status`, which is what `/status` is FOR — has no
way to learn the outcome.

It is **not** a QoS defect and phase-455 W5 does not fix it. W5 made the zenoh
`/status` publisher serve TRANSIENT_LOCAL for real, and that mechanism is
verified working: a late-joining stock TRANSIENT_LOCAL reader, attaching three
seconds after the goal terminated with nothing publishing in between, receives
the publisher's retained last sample. What it receives is `status_list: []`,
faithfully, because that is the last sample the server published. No durability
setting can deliver a sample that was never sent.

## Upstream, for comparison

`rcl_action` keeps a terminated goal in its goal handle array and publishes the
status array on every transition INCLUDING the terminal one; the goal leaves the
array later, via `rcl_action_expire_goals` against `result_timeout` (15 minutes
by default). So a client reading `/status` sees ACCEPTED → EXECUTING →
SUCCEEDED, and a late joiner with the transient-local profile the action
protocol mandates sees the terminal state.

## Fix shape — a decision, not a patch

This issue does not pick one; both change what an image's `MAX_GOALS` slots hold.

1. **Publish the terminal status before removing the goal.** One statement
   moved. It makes a live subscriber see `status: 4`, and it does NOT fix the
   late joiner: the empty array still goes out afterwards and is what gets
   retained.
2. **Keep terminated goals in the status array until their result is
   reclaimed.** `publish_status_array` would iterate `active_goals` +
   `completed_results` (which already carries `goal_id` and the terminal
   `status`), so the retained sample is the terminal one and a late joiner gets
   what the profile promises. `STATUS_ARRAY_BUF` is 512 bytes against a
   `MAX_GOALS` of 4, and a `GoalStatusStamped` is ~32 bytes, so eight entries
   fit. The cost is that a terminated goal stays visible for as long as its
   result does, which is closest to upstream's behaviour but is not the same
   rule as upstream's timeout.
3. **Implement `result_timeout` expiry**, i.e. upstream's rule. The most
   faithful and the most work; it needs a clock in the action core.

Nothing here is safe to pick without deciding what a nano-ros action server
promises about a terminated goal's visibility, which is a design question that
belongs with issue 0902's owner.
