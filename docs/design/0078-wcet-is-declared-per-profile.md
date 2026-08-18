---
rfc: 0078
title: "An execution-time BOUND is declared per measurement profile — and a measured maximum is not one"
status: Draft
since: 2026-08
last-reviewed: 2026-08
implements-tracked-by: [issue-0404, issue-0259, issue-0403]
amends: []
supersedes: []
superseded-by: null
---

# RFC-0078 — An execution-time BOUND is declared per measurement profile — and a measured maximum is not one

## The problem, stated as the consumer sees it

`ros-launch-manifest-sched` (pinned `v0.1.6`, rev `b9f45f1`) computes chain
feasibility as

```
sampling_cost_ms = Σ over boundary elements (period_ms + exec_ms)
controllable     = max_latency_ms − sampling_cost
```

`MapperPath::exec_ms` is `Option<f64>`, and nothing outside rlm's own tests
ever sets it. Every boundary is therefore counted as **zero execution time**,
and the pinned version says so out loud:

> `ChainFeasibleWithoutWcet { chain, boundaries_without_wcet }`
>
> This is not a scheduling problem — it is an evidence problem. Absent is not
> zero: the verdict is optimistic by an unknown amount, and reporting it as a
> plain `feasible` would claim headroom nobody measured.

So the warning already exists, already names the boundaries it counted as zero,
and already cites issue 0259 as the standing analysis. What is missing is
somewhere to put the answer. This RFC defines that.

It defines a DECLARATION format only. Issue 0403 defines the MEASUREMENT
artifact (`nros.wcet.measurements/1`); the two are different objects and the
distinction is the spine of this design.

## Decision 1 — a WCET is keyed on a named measurement PROFILE

A declaration names a profile; a board selects one.

```toml
[wcet.profiles.stm32f4-168mhz-release]
cpu        = "cortex-m4f"
clock_hz   = 168_000_000
profile    = "release"
```

The alternatives collapse into this one rather than competing with it:

* **Board id** is a profile with exactly one member. Choosing it forbids ever
  sharing a number between two boards that genuinely share a context, and
  forces a re-measurement campaign per board.
* **Platform family** (`cortex-m4f`) is a profile that lies by omission: the
  family spans clock rates differing several-fold, so the number needs a rate
  anyway — at which point the family plus the rate IS a profile, just an
  unnamed one.

Naming it makes the load-bearing claim explicit: **a WCET belongs to a context,
not to code.** A reader can see which context, and a reviewer can reject a
declaration whose profile does not match the hardware it is claimed for.

### The cost, stated

An extra indirection for the simple case. A project with one board writes a
profile with one member and selects it once. That is the price of not having to
migrate every declaration the first time a second board appears.

## Decision 1b — a measured maximum is EVIDENCE, not a bound

Revised 2026-08-18 after checking this design against the WCET literature. The
first version of this RFC took `max_cycles` from the bench and converted it
straight into `exec_ms`, calling the result a WCET. That is wrong, and the field
is unambiguous about why.

The longest time observed over N runs is a **high-water mark**, not a bound:

> As it is generally impossible to observe all potential executions of a
> real-world program, this approach cannot provide any guarantees about the
> calculated WCET estimate.
> — Wilhelm et al., *The Worst-Case Execution Time Problem*

Measurement-based estimates are optimistic by an unknown amount. Handing one to
a feasibility check is issue 0259's failure in better clothes: 0259 is an ABSENT
number counted as zero; this would have been an UNDER-estimate counted as
measured, which is worse for being hard to see.

So observation and bound are separate fields, and **observation alone converts
to nothing**:

* `max_observed_cycles` — what the bench saw. Evidence.
* `bound_cycles` — an explicit bound, typically from static analysis, the only
  method that yields a guarantee. Wins when present.
* `margin_percent` — the industrial practice of inflating the high-water mark.
  ~20% is the common figure, and the literature is blunt that it has "very
  little justification … save for historical confidence". Requiring it to be
  WRITTEN DOWN does not make it justified; it makes it visible.

A profile with neither yields no `exec_ms`. The declaration is still valid and
still records the measurement — it simply refuses to let the measurement
masquerade as a guarantee.

`coverage` is a free-text field for what the run exercised, and `None` is an
admission rather than an omission. Measurement-based estimation is only as good
as its coverage, and this tree's bench runs fixed synthetic inputs — so a
reviewer seeing `None` should discount accordingly.

## Decision 2 — declare cycles AND the rate; convert at the boundary of our repo

The consumer's slot is milliseconds and that is a **cross-repo design
agreement**, not something this repo may redefine:

> Unit matches `ChainElement::Boundary::exec_ms` (milliseconds, no invented
> WCET).

The measurement, however, is in cycles, and issue 0403's artifact refuses to
convert: `clock_hz` is null and `convertible_to_time: false`, because the bench
cannot read the part's clock.

So the declaration carries **cycles plus the profile's `clock_hz`**, and
nano-ros converts to `ms` when it populates `exec_ms`. Both halves matter:

* Declaring `ms` directly would force the conversion at the point with the
  LEAST information — a human transcribing a bench log, with the clock rate
  held in their head. That is where an invented rate enters, which is the
  failure issue 0404 exists to prevent.
* Declaring cycles ALONE would hand rlm a number in a unit its schema does not
  accept, and rlm has no way to learn the rate.

The conversion is therefore a named, testable step inside this repo, sitting
between two formats that each refuse to guess.

This shape has independent precedent. Eclipse AMALTHEA/APP4MC declares
execution demand in *ticks* — "a generic abstraction of time for software,
independent of hardware … one may think of ticks as clock ticks or cycles" —
and "to obtain execution times, the number of ticks is divided by the individual
frequency of each processing unit". It also carries lower/average/upper bounds
per runnable, where this schema carries min/max observed plus an explicit bound.

### Consequence

A profile without `clock_hz` cannot produce an `exec_ms`. That is correct and
must stay loud: the declaration is still valid, still auditable, and simply not
convertible — the same distinction 0403's artifact already draws.

## Decision 3 — declarations are per BOUNDARY, at the consumer's identity

rlm reports missing WCETs as `boundaries_without_wcet: Vec<String>`, each entry
a `node/path`. A declaration keys on exactly that:

```toml
[wcet.profiles.stm32f4-168mhz-release.boundaries]
"perception_node/on_scan" = { min_cycles = 41_120, max_cycles = 68_940, iterations = 1000 }
```

Two properties follow, and both are the point:

* the warning and the declaration join on the same string, so "what is missing"
  and "what is declared" are answerable by set difference rather than by
  judgement;
* the schema does NOT define how primitives compose into a callback. Issue 0404
  calls that "a much larger claim", and it is: 0403's bench measures
  `crc32`, CDR serialize, `SafetyValidator::validate`, and a callback is not
  their sum without a model of the callback. Those numbers remain diagnostics.

`max_cycles` is what becomes `exec_ms`. `min` and `iterations` are carried for
audit — a max with no spread and no sample count is not reviewable.

## Decision 4 — provenance travels with the number, and staleness is loud

Each declaration carries the conditions 0403 already emits, because a number
whose origin is unknown cannot be invalidated:

```toml
measured_at_commit = "a1b2c3d4e5f6"
counter_valid      = true
source             = "nros.wcet.measurements/1"
```

`counter_valid` is carried deliberately. 0403's bench refuses to emit
measurements when the cycle counter is dead, so `false` should be unreachable
through the sanctioned path — carrying it anyway means a hand-written or
hand-edited declaration cannot quietly claim to be measured.

**Staleness is the harder half.** A WCET measured three commits ago may or may
not still describe this callback, and nothing in a file can know. This RFC does
NOT claim to solve that. It requires only that the commit is recorded, so a
consumer CAN compare and a reviewer CAN ask. A mechanism that decides
automatically when a declaration has expired is deliberately out of scope; the
weaker requirement is one this repo can actually keep.

## The invariant that outranks every decision above

**Absent stays representable, and absent stays the DEFAULT.**

No boundary acquires a WCET by omission, by inheritance from another profile,
or by falling back to a family default. A schema that requires every boundary
to carry a number gets zeros written into it by hand — and then the tree is
back where 0259 found it, with the added problem that the zeros are signed by a
developer and look like evidence.

`ChainFeasibleWithoutWcet` firing is the CORRECT output for an undeclared
boundary. This design's success condition is that the warning names fewer
boundaries over time, never that it stops firing.

## Worked example — SYNTHETIC, and it has to be said plainly

The numbers below are **fabricated**. No run has ever produced a
`nros.wcet.measurements/1` artifact: QEMU does not implement DWT cycle
counting so the bench refuses there, and this tree has no hardware lane. The
example demonstrates the FORMAT and the conversion arithmetic. It is not
evidence that a measurement has ever flowed end to end, and it must not be
cited as such.

```toml
[wcet.profiles.stm32f4-168mhz-release]
cpu       = "cortex-m4f"
clock_hz  = 168_000_000
profile   = "release"
measured_at_commit = "a1b2c3d4e5f6"
counter_valid      = true
source             = "nros.wcet.measurements/1"

margin_percent     = 20.0   # inflates the high-water mark into a bound
coverage           = "fixed synthetic inputs; worst path not established"

[wcet.profiles.stm32f4-168mhz-release.boundaries]
"/perception_node/on_scan" = { min_observed_cycles = 41_120, max_observed_cycles = 68_940, iterations = 1000 }
```

Selected by a board, `/perception_node/on_scan` yields

```
bound  = ceil(68_940 * 1.20)            = 82_728 cycles
exec_ms = 82_728 / 168_000_000 * 1000   = 0.4924 ms
```

Drop `margin_percent` and the same declaration yields NO `exec_ms` — the
observation is recorded and the boundary stays in
`ChainFeasibleWithoutWcet`.

which populates that boundary's `MapperPath::exec_ms`. Every other boundary in
the system stays `None`, and `ChainFeasibleWithoutWcet` continues to name them.

## What this RFC does not decide

* ~~**The file's location and how a board selects a profile.**~~ SETTLED
  2026-08-18 — see "Where a declaration lives" below.
* **The Rust type and its validator.** Deliberately a separate work item so
  this design is reviewable before an implementation makes it expensive to
  revert.
* **Automatic staleness invalidation** — see Decision 4.
* **Whether primitives can ever compose into a callback.** Out of scope, and
  the reason granularity is per-boundary.

## What the literature says this design still cannot claim

Even with a margin, this is a measurement-based estimate, and no margin makes it
a guarantee. The honest description of what a declaration produced this way
carries is "a high-water mark inflated by a number someone chose". Systems that
need a real bound need static analysis (`bound_cycles`), and this schema has a
field for that precisely so the stronger claim has somewhere to go.

Boundary keys are `<node_fqn>/<path_name>` where the FQN is a ROS name — it
begins with `/` and may carry namespaces, so `/perception/front/on_scan` is node
`/perception/front`, path `on_scan`.

## Where a declaration lives (added 2026-08-18)

Both halves are a `[wcet]` section in `system.toml`:

```toml
[wcet.profiles.stm32f4-168mhz-release]
cpu                = "cortex-m4f"
clock_hz           = 168_000_000
profile            = "release"
measured_at_commit = "a1b2c3d4e5f6"
counter_valid      = true
source             = "nros.wcet.measurements/1"
margin_percent     = 20.0
coverage           = "fixed synthetic inputs; worst path not established"

[wcet.profiles.stm32f4-168mhz-release.boundaries]
"/perception/on_scan" = { min_observed_cycles = 41_120, max_observed_cycles = 68_940, iterations = 1000 }

[wcet.select]
flash-stm32f4-disco = "stm32f4-168mhz-release"
```

**Selection is keyed by deploy-target name, not written inside
`[deploy.<target>]`**, and that is a constraint rather than a preference:
`DeployTarget` is a re-export of `ros_launch_manifest_model::system_config::DeployBlock`,
an upstream type carrying `deny_unknown_fields`. A field added there would have
to land in another repository first. Keying a map by target name keeps "a board
selects a profile" true without that dependency.

### Two hard errors, because the alternative is a silent zero

`SystemToml::wcet_profile_for(target)` returns `Ok(None)` for a target with no
`[wcet]` section and for one with no `[wcet.select]` entry — absent is the
default and stays silent. Everything else is an error:

* **`[wcet.select]` naming a profile that does not exist.** Almost always a
  typo, and a typo must not read as "this board has no measurements". A silent
  `None` here would put the model back to counting every boundary as zero,
  which is issue 0259 with the evidence removed.
* **A selected profile that fails `validate()`.** A declaration that exists and
  cannot be believed stops the build rather than evaporating.

Note that `system.toml` carries `deny_unknown_fields`, so before this section
existed a `[wcet]` block was a hard parse error: the authoring path did not
merely lack a reader, it was actively rejected.

## Open question this design cannot close by itself

The keying decision assumes a human can tell whether a profile matches the
hardware in front of them. Nothing here verifies that: a declaration claiming
`clock_hz = 168_000_000` on a part running at 84 MHz is well-formed and wrong,
and would halve every derived `exec_ms`. Detecting it needs the runtime to
report its own clock — which is exactly what 0403's bench could not do, and is
the same gap that leaves `convertible_to_time: false` in the artifact. Any
future work that gives the platform a clock-rate query closes both at once.
