---
id: 1756
title: "The native async action client gets its goal accepted and then never resolves the awaited result"
status: open
type: bug
area: [rmw, testing]
severity: medium
found: 2026-10-08
related: [1013, 1026]
---

## Measured

`run-matrix` run **37685900447** on `main` (workflow_dispatch, 2026-10-07 20:57Z),
job **113036322535** `tier 1 (cells)`. This was the first tier-1 run on the
self-hosted runner. The test failed after 40.3 s:

```
nros-tests::native_async_roundtrip_e2e native_async_action_client_awaits_goal_and_result
panicked at packages/testing/nros-tests/tests/native_async_roundtrip_e2e.rs:126:5:
async action client accepted the goal but never resolved its awaited RESULT — the .await path
stalled after acceptance (feedback stream or get_result), which the old goal-acceptance-only
assertion reported as a pass.
[INFO] action-client-async: Sending goal
[INFO] action-client-async: Goal accepted by
```

The client log also shows that every action client endpoint was granted less
history depth than it asked for:

```
[WARN] nros: qos: client '/fibonacci/_action/send_goal' asked for KEEP_LAST(10); this image's receive ring holds 4. Granting 4 …
[WARN] nros: client request publisher `/fibonacci/_action/get_result`: asked for history depth 10 and the backend granted 4.
```

This is one of three "real failures" in that run's junit rewrite. The other
two are the `~/.cache` permission error (issue 1755) and a PX4 fixture that the
lane never built (issue 1685).

## What it is NOT

- **Not a skip.** The junit rewrite counts it as a real failure, and it ran for
  40 s against a live router.
- **Not a known flake.** No open issue names this test. Its assertion is the
  strengthened one: it now requires the awaited result, where it used to accept
  goal acceptance alone.

## Cause (measured)

Neither the `.await` path nor the `get_result` reply. The example client
(`examples/linux/rust/action-client-async/src/main.rs`) sent `get_result` only
after it had seen the LAST feedback sample: its `while let Some(..) =
stream.next().await` loop broke only on `feedback.sequence.len() > order`.
Feedback is lossy:

- In its default unpaced mode the server (`examples/linux/rust/action-server`)
  publishes all 11 feedback samples in one tick, then completes the goal.
- The client's feedback subscriber ring holds 4
  (`SUBSCRIBER_RING_DEPTH_DEFAULT`), and when it is full the producer drops the
  NEWEST sample (`nros-rmw-zenoh` `shim/subscriber.rs`).
- If the spin task does not drain the ring during the burst, the final sample
  is lost, the stream waits forever, `get_result` is never sent, and the test
  times out at 40 s.

The sync `action-client` passes because it is callback-driven and its result
does not depend on feedback.

Measured by hand (zenohd + `action-server` + `native-rs-async-action-client`
on unique ports, with a fixture build loading the host), 8 runs: 2 hung. One
received 4 samples (`last=[0, 1, 1, 2]`, exactly the ring depth). The other
received 9 and missed the len-11 sample. Every server log showed all 11
`Publish feedback` lines and `Goal succeeded`.

Raising `ZPICO_SUBSCRIBER_RING_DEPTH` only narrows the window: 11 samples is
still more than 10 slots, and draining depends on timing.

## Fix

The client stops reading feedback when the stream has been quiet for 1 s, then
always awaits `get_result`, which is the authoritative end of the goal.
Feedback is lossy in ROS 2 too. With the fix, the same hand-run harness passed
12 of 12, including runs that dropped the final sample.

Whether the library should change as well (a latest-wins feedback ring, or an
API that waits on feedback and result together) is a separate design question,
not filed here.

## What would close it

The test passes on two consecutive self-hosted tier-1 runs, and the cause above
is named.

## 2026-10-11 — recurred on the next tier-1 run, and is now its only real red

`run-matrix` run **37945434581** (job 114142673988, head `2b8153621`): the
same test failed after 40.354 s, the lane's ONLY real failure of 2767 (181
others were `[SKIPPED:lane]` rewrites). With issues 1684/1685 resolved on that
run, this is what stands between the tier-1 job and a green verdict.
