---
id: 691
title: "`test_ros2_action_xrce_client` slept 6 s instead of waiting for a marker the server already printed, so a loaded host lost the goal and waited out a 20 s budget"
status: resolved
type: bug
area: testing/rmw-xrce
related: [issue-0672, issue-0480]
---

## Symptom

In a full tier-1 sweep:

```
TRY 1 FAIL [  29.987s] nros-tests::xrce_ros2_interop test_ros2_action_xrce_client
TRY 2 PASS [  11.968s] nros-tests::xrce_ros2_interop test_ros2_action_xrce_client
```

Solo it passed 3/3 at ~10.6 s, which reads as the documented full-sweep load
flake — retry and move on.

## It was not generic load

The test's budget is `6 s sleep + 20 s collect_until + 2 s drain`. The failing
attempt burned all of it, so `collect_until` waited its full 20 s and never saw
the result line. Solo, the work after the sleep takes ~4 s. Something made a 4 s
wait exceed 20 s — a 5x stretch that load alone does not explain.

The cause is in the sleep, not the wait:

```rust
// … 3s was too tight under test load and intermittently missed the match; 6s lands reliably.
std::thread::sleep(Duration::from_secs(6));
```

Three facts compound:

1. **The window is real.** rclpy must import, construct 5 action entities and
   announce them over DDS before the client's `send_goal` fires.
2. **The client cannot recover.** The nano-ros action client sends its goal ONCE
   — no `wait_for`, no retry. Miss the match and the goal is silently dropped.
3. **The failure surfaces 20 s later and 6 s away from its cause.** The client
   then waits out its whole budget for a result that can never arrive, so the
   report is a timeout in the client with nothing wrong in it.

So the flake is a race whose window is a hardcoded constant, and the comment
records a previous bump of that same constant (3 s → 6 s) — tuning the window
rather than removing it. Same shape as issue 0672 ("a readiness wait that cannot
observe readiness is a sleep") and issue 0480.

## The signal was already there

`action_server_fibonacci_with_domain`'s script has always printed:

```python
print('SERVER READY', flush=True)
```

under `python3 -u … 2>&1`. Nothing read it. The test captured that stream only at
the END of the run, to look for `SERVER DONE`.

## Fix

Wait for `ROS2_ACTION_SERVER_READY` (a constant, not a literal) with a 30 s
ceiling, then a 1 s settle for DDS propagation — a genuine settle, not a
stand-in for a signal. `wait_for_output_count` returns on the marker and leaves
the process running, so the terminal drain still works.

The result is faster AND correct: **6.1 s, down from 10.6 s**, because it now
waits the ~4 s the server needs instead of a fixed 6 — and it does not expire
when a loaded host needs 9.

## Verified

3/3 solo at 6.09–6.11 s; full `xrce_ros2_interop` suite 9/9.

## Not fixed here

Six sibling `sleep(1)` calls remain in this file after other server spawns. They
did not produce this failure and each needs its own look at whether a marker
exists to wait on — recorded rather than swept blind.
