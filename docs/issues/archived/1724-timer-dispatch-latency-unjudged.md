---
id: 1724
title: "max-latency-runtime never judged a timer callback fired by the
  trigger-miss sweep: a node that also subscribes ran its timers untimed"
status: resolved
type: bug
area: [core, diagnostics]
severity: high
found: 2026-10-06
related: [phase-474, issue-1715, rfc-0052]
resolved_in: "phase-474 I6 (feat/474-timer-latency)"
---

## What happened

On the safety island (S32K344, phase9-W4) a commanded 250 ms overrun of the
handler's timer tick, after arming, stored `timer-overrun-runtime` and
`release-jitter-runtime` verdicts but no `max-latency-runtime` verdict, on a
tick that published `mrm_state`, `takeover_request_state` and
`hazard_lights_cmd`, monitored at 206, 206 and 100 ms. Every contracted path
on the island is timer-driven, so its latency rows were never judged at run
time.

## Cause

`Executor::spin_once_capturing` (`nros-node` `executor/spin.rs`) evaluates
the executor trigger before dispatch. The default `Trigger::Any` passes only
when some non-timer entry has data. When it does not pass, due timers still
fire, in the sweep under "Timers still need delta accumulation even when
trigger doesn't pass": a bare `try_process` per timer entry, with no clock
read and no `attribute_latency`. Only the EDF and FIFO drains timed a
dispatch. A node that subscribes (the island's handler does) and has no
sample this spin therefore fires its timers in the sweep, and their elapsed
time reached no rule. The host T4 test passed because its node had no
subscription, so `non_timer_mask == 0` made `Trigger::Any` pass and the timer
fired in the timed FIFO drain.

## Resolution (phase-474 I6)

The sweep reads the clock before and after each timer's `try_process`, when
a latency contract exists and a clock is injected, and charges the elapsed
time through `attribute_latency` to the monitored publishers whose count
advanced during that dispatch -- the same attribution as the drains. Cost: a
timestamp pair and a fixed-size count snapshot on the stack per timer entry
per sweep; nothing allocates. Test:
`t4_a_timer_overrun_beside_an_idle_subscription_is_judged` (fails before the
fix with no verdict, passes after with one `max-latency-runtime` verdict on
the timer's publisher, measured >= declared).

The measured span is callback entry to exit (a service call made inside the
callback is inside it), not the timer's release jitter, which
`release-jitter-runtime` covers. Documented in `monitor.rs` and RFC-0052.

Not changed here: the sweep still skips the sporadic budget accounting, the
`deadline-miss-runtime` check and the alive-supervision count that the drains
apply to a dispatch, and it returns before the post-dispatch rules of that
spin (timer overrun, release jitter, stack headroom, alive supervision, the
drain-and-report hook); those run at the next spin whose trigger passes.
