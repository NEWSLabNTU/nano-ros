---
id: 2777
title: "A threadx_linux image built with connect_forever no longer outlasts a router that starts after it: it boots and never comes up"
status: open
type: bug
area: [threadx, zenoh, ci]
severity: high
found: 2026-11-21
related: [2207, 2760, 2624, 2706]
---

## What fails

`rtos_e2e::test_rtos_session_open_outlasts_a_late_router::platform_1_Platform__ThreadxLinux`
in `merge-runtime.yml`, lane `merge`, shard `threadx_linux`:

> threadx_linux late router: the image started 3s before its router and never
> reported `Application setup complete` — an image built with connect_forever
> did not outlast the gap, so its open is not retrying (issue 2207).

The listener prints its banner, the network bring-up and `[INFO] nros: boot`,
and nothing after: it is stuck in, or before, the session open. No error is
printed. On a board this is an image that boots and silently never joins the
graph whenever its router comes up second.

## A regression, not a flake

| run | main | date | this test |
| --- | --- | --- | --- |
| 44030478812 | cf3589e386 | 2026-11-17 | PASS |
| 44090176782 | 2cfe9f0094 | 2026-11-17 | PASS |
| 44721090137 | 0d9429d990 | 2026-11-20 | FAIL |
| 44768834808 | b13a5d4b18 | 2026-11-20 | FAIL (the shard's only real failure) |

Read from each run's `merge-runtime-merge-threadx_linux-junit` artifact.

## Where to look

`2cfe9f0094..0d9429d990` is 451 commits. By subject, most likely first:

1. `1a896a6722` fix(smoltcp, zenoh): durations read the monotonic clock
   (issue 2760). The open's retry sleep and its connect budget are durations.
2. `2a0260282f` the ThreadX byte pool is derived from measured working sets
   (issue 2624). An open that retries allocates per attempt and could exhaust
   a tighter pool without saying so.
3. `8cd0dfd9a7` an interrupt wakes a parked ThreadX executor (issue 2706).
4. `6229085857` every image serves the stock node interfaces by default: more
   entities to declare once the session is open.

Not bisected: threadx_linux needs the veth bridge, which the host that found
this cannot set up.

## Closes when

The test passes again on the merge lane, with the cause named here.
