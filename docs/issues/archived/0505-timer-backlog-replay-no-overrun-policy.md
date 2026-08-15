---
id: 505
title: "Periodic timers replay the whole backlog after a stall — 6 control callbacks 88 us apart — with no overrun policy and no overrun counter"
status: resolved
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

## Exploration findings (2026-08-11)

The Skip/CatchUp mechanism landed (default Skip, saturating `overruns`,
`Executor::set_timer_overrun_policy` / `timer_overruns`). Four things
found while working through the agenda; two change what the remaining
work should be.

### 1. CatchUp does not merely burst — it launders the failure

Measured A/B on the QEMU mps2-an385 lane, same tree, only the default
policy flipped, 2 kHz inbound flood, 2 runs each (the flood level that
reliably produces transport-band stalls, issue #506):

| default policy | achieved ctrl rate | replay activations | stalls/run |
|---|---|---|---|
| CatchUp | **100.03 Hz** (declared 100) | 275/run | 13.0 |
| Skip | 90.8-93.0 Hz | 8/run | 12.0 |

Under CatchUp the island stalls for up to 611 ms at a time and the
achieved publish rate is still exactly 100.03 Hz, because every missed
activation is replayed later in the same window. `check_rate` in
`executor/monitor.rs` counts publishes over a ~5 s window, so
`rate-hierarchy-runtime` is STRUCTURALLY blind to a stall under
CatchUp: the failure is invisible to the one runtime rule that ought to
see it. Under Skip the same stalls show up as a 7-9% rate deficit.

This is a stronger argument for the Skip default than "bursts are
surprising": CatchUp makes the existing contract monitor report health
during a fault. Recommend keeping Skip as the default and treating
CatchUp as an explicitly-requested behavior for counters/accumulators.

Caveat for the monitor discussion below: a 7-9% deficit only trips
`rate-hierarchy-runtime` if the declared minimum sits within ~10% of
nominal. A single isolated stall (20 missed activations of a 100 Hz
loop in a 5 s window = 0.4%) will not trip it at any sane threshold.
The rate rule catches sustained degradation; it is not a substitute for
an explicit overrun signal.

### 2. Nothing in-tree depends on CatchUp semantics

Audited every `register_timer` / `create_timer` site in `packages/` and
`examples/`: the users are demo talkers and test binaries publishing at
200-1000 ms, plus the C and C++ shims. None counts activations or
accumulates per-tick state, so none is behaviorally affected by the
default flip except in the direction it wants (no burst). The
`register_timer` callback signature stays `FnMut()`, so nothing
recompiles differently either.

### 3. The ms granularity is worse than "coarse" — it is silent

- `TimerDuration` stores milliseconds, and `from_micros` divides by
  1000. `from_micros(500)` yields a ZERO period, which the dispatcher
  treats as "fire every spin"; `from_micros(1_500)` silently becomes
  1 ms, a 50% rate error. No error, no warning.
- The C ABI is already nanoseconds (`nros_timer_t.period_ns`), and
  `nros-c/src/executor.rs` truncates `period_ns / 1_000_000` at the
  boundary, rejecting sub-ms with `NROS_RET_INVALID_ARGUMENT`. So the C
  surface promises ns, delivers ms, and the two language paths disagree
  on what a 500 us timer does (C rejects, Rust silently free-runs).
- A period that is not a multiple of the tier's spin period quantizes
  deterministically. Measured on a 33 ms timer in a 5 ms-spin tier: the
  period alternates 35 ms (444 samples) / 30 ms (288), mean 33.07 ms.
  The long-run rate is right, the individual periods are never right,
  and the pattern is fully predictable at resolve time.

Widening cost is small and mostly mechanical. `CallbackMeta.try_process`
is `unsafe fn(*mut u8, u64)` — the unit is a convention, not a type, so
changing it is a rename plus one arithmetic site: 22 `delta_ms: u64`
signatures in `arena.rs` (all but `timer_try_process` ignore the
argument), 18 references elsewhere. It also SIMPLIFIES `spin.rs`, which
currently keeps a `spin_residual_us` accumulator solely to convert its
already-microsecond delta into milliseconds. Suggest doing the widening
as its own change, then having `from_micros` stop lying.

### 4. Where the overrun counter should surface

`executor/monitor.rs` already has the shape: a `Violation { rule, fqn,
measured, declared }` in play_launch's rule-id vocabulary, drained into
a ring that the entry glue hands to `nros-diagnostics`. Timers are not
endpoints, so a per-endpoint `MonitorSpec` row does not fit, but
`deadline-miss-runtime` already precedents reporting with the
SchedContext name as `fqn`. Proposal: `timer-overrun-runtime`, `fqn` =
the bound SC/tier name, `measured` = overruns accrued in the window,
`declared` = tolerated count (0 by default). That needs a
window-delta baseline next to `MonitorState.count_at_window_start`,
mirroring the rate rule.

## Status (2026-08-11): all three pieces landed

1. **Overrun policy** — `TimerOverrunPolicy::{Skip,CatchUp}`, Skip the
   default, saturating `overruns` counter, phase-preserving remainder.
2. **Microsecond resolution** — the truncation was at THREE boundaries,
   not one: `TimerDuration` stored ms (so `from_micros(500)` produced a
   zero period that free-runs, `from_micros(1_500)` a 50% rate error);
   the declarative path round-tripped through `EntityMetadata.period_ms`;
   and `nros-c` took ns on the ABI then truncated to ms, rejecting
   sub-ms periods the Rust API silently accepted. All three now carry
   microseconds; `spin_residual_us` is deleted (it existed only to
   truncate an already-microsecond delta). Measured on the FreeRTOS
   lane, a mid tier declared at 33.333 ms: rate error -0.99% before,
   +0.01% after.
3. **`timer-overrun-runtime`** — dropped activations now reach the same
   violation ring as the rate/age/latency/deadline rules, reporting the
   delta since the last check. Runs after dispatch (the windowed rules
   tick before it) so a stalling tier reports in the same cycle.

Remaining, deliberately out of scope here:

- **No board entry glue drains violations** into `nros-diagnostics` on
  the FreeRTOS lane, so overruns are reportable but not yet reported on
  target. That is the same gap every other runtime rule has on this
  lane; worth its own issue rather than being smuggled in here.
- **Policy is not declarable** in launch-level scheduling metadata —
  it is an executor-level call today. Whether a tier dim should carry
  it (`real_time` class implying Skip, say) belongs with the contract
  schema discussion, not the executor.
- **Period/spin quantization is still silent.** A declared period that
  is not a multiple of its tier's spin period quantizes to the spin
  grid: 33 ms on a 5 ms spin alternates 35/30 ms, mean 33.07. The
  long-run rate is right and no activation is dropped, so no rule
  fires — correctly, but the jitter is entirely predictable at resolve
  time and would be better as a resolver warning than a surprise on
  target. Filed as a separate concern.


## RESOLVED 2026-08-15 — the last piece was the writing-down

The three code pieces landed 2026-08-11 (policy + counter, microsecond
resolution, `timer-overrun-runtime`). What remained was phase-358 W2's other
half, which the code could not satisfy: *"the policy is written down with its
rationale ... and the default is stated in the docs."* Until now the rationale
lived only in this issue and in doc-comments, which is exactly the "chosen
implicitly by the implementation" outcome W2 set out to avoid.

* **RFC-0002 § 4.4a** (RT execution model) now carries the policy, the `Skip`
  default, and the argument that actually justifies it — the measured A/B
  showing `CatchUp` reporting 100.03 Hz on a declared 100 Hz loop while the tier
  stalled for up to 611 ms, i.e. the rate monitor reporting health during the
  fault it exists to catch. The rclcpp / Zephyr `k_timer` precedents, the
  observability wiring, the surface decision (queryable counter, `FnMut()`
  unchanged), and the microsecond resolution history are recorded with it,
  including the caveat that a 7–9 % deficit only trips the rate rule when the
  declared minimum sits within ~10 % of nominal.
* **book/src/user-guide/configuration.md** states the user-facing default:
  coalesced not replayed, phase preserved, `CatchUp` opt-in for
  counters/accumulators, watch `timer-overrun-runtime` rather than a publish-rate
  check.

Verified against the code rather than against this issue's own summary before
writing either: `#[default] Skip`, `elapsed_us %= period_us` (phase-preserving),
saturating `overruns`, microseconds end to end including the C ABI's ns→µs
conversion. `cargo test -p nros-node --lib overrun` — 18 passed.

**W2's acceptance is met in substance but not in sequence, and that is worth
stating.** It asked for the policy to be written down *before* it was coded. It
was coded first, on 2026-08-11. Nothing here can undo that ordering; what this
does is stop the decision living only in the implementation.

### Deliberately still out of scope

* No board entry glue drains violations into `nros-diagnostics` on the FreeRTOS
  lane, so overruns are reportable but not yet reported on target — the same gap
  every other runtime rule has there.
* The policy is not declarable in launch-level scheduling metadata; whether a
  tier dim should carry it belongs with the contract schema.
* Period/spin quantization is still silent (33 ms on a 5 ms spin alternates
  35/30, mean 33.07). No activation is dropped so no rule fires, correctly, but
  it is predictable at resolve time and would be better as a resolver warning.
