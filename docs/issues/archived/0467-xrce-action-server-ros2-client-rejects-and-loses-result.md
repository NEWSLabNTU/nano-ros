---
id: 467
title: "`test_xrce_action_ros2_client`: the nano XRCE action server rejected ~half the ROS 2 client's goals — the goal type was decoding the UUID"
status: resolved  # fixed upstream by #0461; verified 2026-08-07
type: bug
area: rmw
related: [issue-0461, issue-0448, issue-0422]
---

## Symptom

`xrce_ros2_interop::test_xrce_action_ros2_client` — the REVERSE direction of
#0448 (nano-ros is the action SERVER, a real ROS 2 client sends the goal).
Failed 3 of 3 solo runs on an idle box, in two alternating modes:

```
Goal was rejected.
  → accepted=false feedback=false result=false
Goal accepted with ID: 4d5f0529025a4cefb4cddd1ff07c3daf
  → accepted=true  feedback=true  result=false
```

## Root cause — already fixed upstream as #0461

The typed goal callback handed `CallbackCtx` the WHOLE `SendGoal` request,
`[CDR header][goal_id uuid(16)][goal fields]`, and `CallbackCtx::message::<M>()`
skipped only the 4-byte encapsulation. So the goal type decoded its fields
starting at the UUID.

The example server accepts on `order >= 0`:

```rust
let order = ctx.message::<FibonacciGoal>().map(|g| g.order).ok();
let accept = order.map(|o| o >= 0).unwrap_or(false);
```

With a **nano-ros** client the UUID begins with a goal counter, so `order`
always looked like a small positive number and the bug was invisible — which is
how it survived. With a **ROS 2** client the UUID is RANDOM, so `order` was a
random `i32`: negative roughly half the time → "Goal was rejected". That is
exactly the ~50% rejection rate measured here, and it explains why the two modes
alternated run to run rather than being deterministic.

#0461 fixed it in `CallbackCtx::message()` by skipping `GOAL_UUID_LEN` when the
callback carries a goal decision, matching the typed `try_accept_goal` path
which had always skipped it.

## Verification

On the tree with #0461 applied, 3 consecutive solo runs pass (18.8 s, 12.2 s,
11.0 s) — against 3/3 failures before. Run three times deliberately: a single
green run is not evidence about an intermittent test.

## Notes

Filed 2026-08-07 from observations on a tree that predated #0461, so this issue
is a duplicate of it, kept for the independent derivation of the same root
cause (the random-UUID-vs-counter asymmetry is the reason a nano↔nano test can
never catch this class — the same blind spot #0453 records for goal payloads).

The `result=false` mode needed no separate explanation: a goal accepted with a
garbage `order` still ran, but the run was never the one the test asked for.
