---
id: 467
title: "`test_xrce_action_ros2_client`: the nano XRCE action server rejects most goals, and when it accepts, the result never reaches the ROS 2 client"
status: open
type: bug
area: rmw
related: [issue-0422, issue-0448, issue-0462]
---

## Symptom

`xrce_ros2_interop::test_xrce_action_ros2_client` — the REVERSE direction of
#0448 (nano-ros is the action SERVER, a real ROS 2 client sends the goal).
Fails 3 of 3 solo runs, each of which internally retries:

```
Goal was rejected.
  → accepted=false feedback=false result=false
Goal accepted with ID: 4d5f0529025a4cefb4cddd1ff07c3daf
  → accepted=true  feedback=true  result=false
```

Two distinct modes, both present across runs:

1. **Goal rejected** — the ROS 2 client's `send_goal` comes back rejected, so
   nothing else runs.
2. **Goal accepted, feedback flows, RESULT never arrives** — `accepted=true
   feedback=true result=false`. This one is the more informative: the goal
   crossed, the server ran and published feedback, and only the `get_result`
   reply is missing.

Mode (2) rules out a pure discovery/transport failure and points at the
server-side `get_result` path or its reply framing.

## Frequency

Not deterministic. Observed:

- 1 solo run PASSED (16.6 s) earlier the same session
- 1 solo run FAILED (98.7 s)
- 3 consecutive solo runs FAILED

So it is mostly-failing with an occasional pass, NOT a load-only flake — it
fails with the machine otherwise idle. That distinguishes it from
`large_msg::test_xrce_e2e_integrity`, which passes solo consistently and only
fails inside the full parallel sweep.

## Correction to a claim made in #0462

#0462's table listed this test as "passes" solo. That was written from a SINGLE
passing run and is wrong: three subsequent solo runs fail. #0462 has since been
resolved and archived upstream (its own defect was real but already fixed in
source — what it captured was a stale fixture), so the correction lives here.

This is the second time in one session that a single green run produced a false
conclusion (the first was #0447, nearly misfiled as a stale fixture). **One run
is not evidence about an intermittent test, in either direction.**

## NOT established: whether this is new

I have not bisected it, and it is not in #0422's baseline list — but that list
was assembled from runs where this test may not have been reached.

The code-path argument says it is unrelated to the #0448/#0447/#0458 work landed
in the same session:

- #0448 changed `nros::send_goal`, which is the action CLIENT path. Here the
  client is ROS 2 and the nano side is the SERVER, which never calls it.
- #0447 changed `LinuxBoard::run_tiers`, which only runs for multi-tier entries;
  this example is a single-tier `run`.
- #0458 changed `nros-cpp`, which a pure-Rust example does not link.

That is an argument, not a measurement. Confirming it needs the pre-fix
`action-server` binary (a worktree at the parent commit, rebuild, re-run) —
worth doing before anyone treats this as a regression OR as pre-existing.

## Reproduce

```console
cargo nextest run -p nros-tests --test xrce_ros2_interop test_xrce_action_ros2_client --no-capture
```

## Notes

Surfaced by the tier-1 `just ci` run after the NuttX SDK was provisioned
(2026-08-07). The same run's other failures are accounted for: #0460
(`entry_matrix` nuttx-arm/rust, already open), #0462 (`workspace_features`), and
`large_msg::test_xrce_e2e_integrity` (sweep-only flake, passes solo).
