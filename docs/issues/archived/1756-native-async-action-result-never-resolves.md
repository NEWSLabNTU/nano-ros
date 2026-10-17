---
id: 1756
title: "The native async action client gets its goal accepted and then never resolves the awaited result"
status: resolved
type: bug
area: [rmw, testing]
severity: medium
found: 2026-10-08
related: [1013, 1026, phase-507]
---

**Home: [phase-507](../roadmap/phase-507-xrce-and-action-client-robustness.md)** — homed 2026-10-17; listed in that phase's "Issues homed here (2026-10-17)".

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

## 2026-10-16 — the fix holds locally; no tier-1 run since it has reached the cells

The fix (`c0d34e3bf6`, PR #1947) is on `main`, but the acceptance above has not
been met because no self-hosted tier-1 run has run this test since it landed.
All nine completed `run-matrix` tier-1 jobs from 2026-10-12 to 2026-10-15
(38213993313 through 38663782341) report `NO VERDICT: stopped in the build`. The latest, 38663782341, stopped at
`artifact-identity-budget did not verify… no build tree at
examples/workspaces/mixed/build-workspace-fixtures`, which has nothing to do
with this test.

Measured by hand instead, at `d10d92ee8d`. The harness runs the test's
scenario: Jazzy `rmw_zenohd` on a private port, `action-server`, then the async
client under a 40 s timeout, built with the same bare `cargo build` the fixture
build uses. Its contended mode pins server, client and two busy loops onto one
CPU, which is the "spin task does not drain the ring during the burst" timing
from the cause above, reproduced deterministically:

| Client | Mode | Result | Feedback samples received |
| --- | --- | --- | --- |
| fixed (`main`) | plain | 20 / 20 pass | 11 |
| fixed (`main`) | one CPU, contended | 20 / 20 pass | 4 in every round |
| pre-fix (the fix's diff reverse-applied) | one CPU, contended | 0 / 6 pass, all hang to the timeout | 4 in every round |

Under contention every round drops the final feedback sample, since only 4 of
11 fit the ring, so every contended round exercises the dropped-sample path.
The pre-fix client hangs on all of them, and the fixed client completes all of
them via `get_result`. That confirms both the cause and the fix. The issue
stays open on its stated acceptance until the tier-1 lane reaches its cells
again.

Found on the way: `nros build` refuses this leaf on a fresh checkout because
four of its crates are in no pinned range. That is issue 1924. It does not
affect the fixture build, which uses bare `cargo build`.

## Resolution (2026-10-18)

The acceptance is met. The tier-1 lane reaches its cells again, and the test
passed on four consecutive self-hosted tier-1 runs, each with the fix
(`c0d34e3bf6`) in its head:

| `run-matrix` run | tier-1 job | runner | result |
| --- | --- | --- | --- |
| 38818350050 | 116843072656 | newslab-server-241 | PASS, 5.760 s |
| 38861323075 | 116936720826 | newslab-118-server | PASS, 1.419 s |
| 38933437082 | 117100086153 | newslab-server-241 | PASS, 4.129 s |
| 38980380118 | 117248584061 | newslab-server-241 | PASS, 1.224 s |

Each is a real run, not a `[SKIPPED:lane]` rewrite: the durations are those
of a client completing its goal, against 40 s for every failure above. The
two runs that failed tier 1 overall failed on other cells. The next run,
39009801514, stopped in the build and has no verdict on this test.

The cause is the one named above. The library-side question (latest-wins
feedback ring, or an API that awaits feedback and result together) is still
not filed, as stated in the fix section.
