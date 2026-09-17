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

**What has to be decided first, because every option is a different answer to
it: for how long does a nano-ros action server promise a terminated goal is
visible on `/status`?** Upstream answers "until `result_timeout` expires it,
15 minutes by default". A fixed-capacity server cannot copy that answer for
free — `active_goals` and `completed_results` are both
`heapless::Vec<_, MAX_GOALS>` with `MAX_GOALS = 4`
(`action_core.rs:215`, `:222`, `:223`) — so visibility competes with capacity
to accept NEW goals. That trade is the decision; the three options below are
where each puts it.

Facts the options rest on, measured in the code rather than assumed:

* `STATUS_ARRAY_BUF` is 512 bytes (`action_core.rs:84`) and a
  `GoalStatusStamped` serialises to roughly 32, so the buffer holds about eight
  entries — `MAX_GOALS` active plus `MAX_GOALS` completed fits with room over.
* `completed_results` already carries both `goal_id` and the terminal `status`
  (`CompletedResultEntry`, `:121`), so option 2 adds no state; it reads state
  nothing currently publishes.
* Nothing in the action core has a clock, which is what makes option 3 the
  expensive one.

### 1. Publish the terminal status before removing the goal

Move one statement: publish, then `swap_remove`.

* **Fixes:** a subscriber attached BEFORE the goal terminates now sees
  `status: 4` instead of jumping from ACCEPTED to an empty array.
* **Does not fix:** the late joiner. The empty array still goes out immediately
  afterwards and is therefore what the transient-local publisher retains, so
  the case phase-455 W5 built the retention mechanism for still reads
  `status_list: []`.
* **Promise it makes:** a terminated goal is visible for one publish, to
  whoever was already listening. That is strictly weaker than the profile the
  action protocol mandates, which exists precisely for the client who was not.
* **Cost:** none. No capacity change, no clock.

Take this only as a stepping stone — it is a real improvement that leaves the
headline symptom in place, so shipping it alone would close the symptom's
visible half and leave 1341's acceptance unmet.

### 2. Keep terminated goals in the array until their result is reclaimed

`publish_status_array` iterates `active_goals` + `completed_results` instead of
`active_goals` alone.

* **Fixes:** both cases. The last sample published carries the terminal status,
  so the live subscriber sees it AND it is what the retained sample holds.
  1341's acceptance is then reachable.
* **Promise it makes:** a terminated goal is visible for as long as its result
  is — which is a real rule a caller can reason about, and it is the closest
  thing to upstream reachable without a clock. It is NOT upstream's rule: the
  lifetime is "until the client fetches the result, or the slot is reused",
  not "15 minutes".
* **Cost:** the status array grows to at most `2 × MAX_GOALS` entries, which
  the 512-byte buffer already holds. No new state, no clock. What it changes is
  what a reader of `/status` infers about liveness: a goal in the array is no
  longer necessarily active, so anything that counted array entries as active
  goals has to read `status` instead. That is one grep, and it is the whole
  hidden cost.

### 3. Implement `result_timeout` expiry — upstream's rule

Keep terminated goals and retire them on a timer, as `rcl_action_expire_goals`
does.

* **Fixes:** both cases, with the same lifetime semantics a ROS 2 client
  already expects, so there is nothing to document as a divergence.
* **Cost:** the action core has no clock. Giving it one is not a small change
  on embedded — it is a per-server time source, a knob for the timeout, and a
  new reason for the core to be driven even when nothing else is happening.
  Every platform pays for it.
* **Note:** this is option 2 plus a retirement policy. Doing 2 first does not
  make 3 harder, and 2's rule is a defensible end state on a device.

### Recommendation, for whoever holds the decision

**Option 2**, unless the answer to the visibility question is "exactly
upstream's". It is the only cheap option that reaches 1341's acceptance, its
cost is bounded by a buffer that already fits, and it leaves option 3 open as a
later refinement rather than foreclosing it. Option 1 is not sufficient alone
and option 3 buys fidelity nobody has yet asked for at a cost every platform
pays.

This remains a design question that belongs with issue 0902's owner; the
recommendation is stated so the decision is a yes or a no rather than a survey.
