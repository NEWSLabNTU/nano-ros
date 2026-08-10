---
id: 505
title: "Periodic timers replay the whole backlog after a stall — 6 control callbacks 88 us apart — with no overrun policy and no overrun counter"
status: open
type: enhancement
area: embedded
related: [issue-0502]
---

## Problem

When an executor tier is preempted long enough to miss several periods
of a periodic callback, the timer fires every missed activation
back-to-back once the tier runs again. Observed on the FreeRTOS
mps2-an385 lane (QEMU, guest-clock timestamps): after a ~200 ms
preemption of the application tiers, a 10 ms control callback fired 6
times at ~88 us spacing (NEWSLabNTU/nano-ros-rt-eval, run
`results/20260810-044653Z`, guest stamps `t_us=6695984..6696429`).

For a control loop this is the wrong semantics on both ends:

- The replayed activations compute with stale inputs and publish a
  burst of 6 commands inside half a millisecond — a downstream actuator
  sees a command rate 100x the declared one, and `dt`-based math (which
  today assumes the nominal period, see issue 0504) integrates 6 steps
  of error.
- The application cannot even tell it happened: no overrun count, no
  "this activation is late by X" on the callback, nothing for a
  monitor to alert on. The stall is only visible to an external
  observer diffing timestamps.

## Fix direction

Two independent pieces:

1. **Overrun policy per timer** — at minimum `CatchUp` (today's
   behavior, correct for counters/accumulators) and `Skip` (coalesce
   the backlog into one activation aligned to the next period, correct
   for control). Default for real-time-class tiers should arguably be
   `Skip`; either way the choice must be declarable, not baked in.
2. **Overrun observability** — a per-timer missed-activation counter
   exposed where the executor's scheduling monitors can read it. The
   on-target monitors currently watch achieved rate at the publish
   site; a replayed burst *satisfies* a rate check while being exactly
   the failure the check exists for. An overrun counter is the cheap,
   unambiguous signal (and unlike period measurement it does not
   depend on clock resolution, issue 0502).

## Discussion agenda (2026-08-11, pre-RFC)

Mechanism, located: `timer_try_process`
(`nros-node/src/executor/arena.rs`) accumulates `elapsed_ms += delta`
and on fire does `elapsed_ms -= period_ms` — the whole stall remains as
backlog, and each subsequent spin pass fires one more callback, which
is exactly the observed back-to-back burst. A `Skip` policy is a
two-line core change (`elapsed_ms %= period_ms` + count the dropped
whole periods); everything worth discussing is semantics and surface.

1. **Default.** Precedents both point away from CatchUp: rclcpp
   advances `next_call_time` past "now" (missed periods are skipped),
   and Zephyr's `k_timer` coalesces expiries into ONE callback carrying
   the expiry count. Proposal: `Skip` is the default for everything —
   CatchUp is the surprising behavior, not the conservative one — with
   `CatchUp` opt-in for counter/accumulator uses. Counter-argument to
   discuss: changing a default under existing users; a middle ground is
   per-tier defaulting (real_time class -> Skip) via the contract.
2. **Phase preservation.** `elapsed_ms %= period_ms` keeps the original
   phase grid (fires stay aligned to the declared cadence);
   re-anchoring to "now" would drift the grid every stall. Propose
   grid-preserving and document it.
3. **Surface.** Three candidate layers, not exclusive: (a) a
   `register_timer` parameter (core, always available); (b) a tier dim
   in the launch-level contract so the policy is declarative and
   auditable; (c) nothing node-visible beyond the counter. The callback
   signature question matters here: Zephyr passes the expiry count into
   the handler — do we extend the callback to `FnMut(u32 /*missed*/)`
   (breaking) or keep `FnMut()` + a queryable counter (non-breaking)?
   Lean: non-breaking counter first.
4. **Observability wiring.** `executor/monitor.rs` already carries the
   node-path budget machinery; a per-timer `overruns` counter wants the
   same treatment (readable by SchedContext monitors, surfaced through
   the existing diagnostics path). Note the interplay: with CatchUp, a
   replay burst SATISFIES the publish-rate monitor; with Skip, the
   missed periods become visible to it. Skip therefore makes the
   existing rate monitoring honest — worth stating in the RFC as a
   correctness argument, and worth checking that transient stalls
   tripping the rate rule is the desired alarm behavior (it is the
   alarm's purpose, but it changes observed alert rates).
5. **Adjacent gap, decide scope.** `TimerEntry` is millisecond-granular
   (`period_ms`, `elapsed_ms`), so sub-ms periods are inexpressible and
   period error carries ms rounding even after the #502 clock fix
   (empirically: a declared 33.333 ms tier ticks at ~34.7 ms on the
   FreeRTOS lane — spin-period + ms quantization, measured while
   validating #502). Decide: widen to us inside #505's change (same
   struct, same tests) or file separately. Widening while already
   touching the struct is cheap; separate keeps #505 reviewable.
6. **Tests.** `timer_try_process(ptr, delta_ms)` takes the delta
   directly, so backlog scenarios are pure unit tests (inject 200,
   assert one fire + N overruns + phase kept). No target runs needed
   except a confirmation pass on the QEMU lane.
