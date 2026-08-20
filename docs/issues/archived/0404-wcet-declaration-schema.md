---
id: 404
title: No schema for declaring a measured WCET — where it lives, what it is
  keyed on, and what makes it trustworthy
status: resolved
type: enhancement
area: orchestration
related: [0259, 0403, rfc-0047, rfc-0060]
---

## Problem

`MapperPath.exec_ms` is `Option<f64>` and, outside rlm's own tests, nothing
ever sets it to `Some(..)`. There is no syntax anywhere in `system.toml`, the
launch tree, or the model for a developer to write down a WCET they measured.

As of rlm v0.1.4 the mapper at least admits this: a chain judged feasible with
missing WCETs raises `ChainFeasibleWithoutWcet` naming the boundaries counted
as zero (issue 0259, step 1). The warning is now the thing with no remedy —
it tells you evidence is missing and offers nowhere to put it.

The design constraint, from the outset: **vacant is the default, and stays
vacant.** A WCET is a property of code on a particular chip at a particular
clock, obtained by measurement. Anything that manufactures one — a default, an
estimate, a value inherited from a different board — reintroduces exactly the
unsoundness 0259 removed, but harder to see, because a declared number carries
more authority than an absent one.

## The questions the schema has to answer

**What is it keyed on?** A WCET measured on an STM32F407 at 168 MHz says
nothing about the same code on a Cortex-M3 in QEMU. Candidates:

- *Board id* — most precise, and the boards that can actually measure are a
  minority (issue 0403: on QEMU the DWT reads zero). Most entries would be
  absent, which is correct but makes the mechanism look broken.
- *Platform family* — fewer entries, but "cortex-m4f" spans clock rates that
  differ several-fold, so the number needs a rate anyway.
- *Named profile* — the declaration names a measurement context
  (`stm32f4-168mhz-release`) and the board selects one. Most explicit about
  the fact that a WCET belongs to a context rather than to code.

**What is the unit?** The mapper works in `ms`; the bench measures cycles.
Converting requires a clock rate, which means either the declaration is in ms
(and the conversion happened where the rate was known) or it is in cycles plus
a rate (and the model does the conversion). The first keeps the model simple;
the second keeps the measurement in the unit it was taken in.

**What granularity?** The mapper wants one `exec_ms` per timer boundary — a
whole callback. The bench measures primitives. Either the declaration is
per-callback and measured as such, or the schema defines how primitives
compose, which is a much larger claim.

**What makes it trustworthy later?** A number with no provenance cannot be
audited or invalidated. At minimum: what was measured, on what hardware, at
what clock, with which build profile, at which commit. And then — does a WCET
measured three commits ago still describe this callback? A stale WCET is worse
than an absent one, because absent is now loud and stale is silent.

## Direction

Nothing here should be designed before 0403 produces an artifact. The schema's
job is to carry a measurement from the producer to the mapper without losing
what makes it meaningful, and it cannot be designed against a producer that
does not yet emit one. Doing it in the other order would produce a schema
shaped around a hypothetical measurement, which is how the keying question
gets answered by guess.

### Unblocked 2026-08-16 — the producer exists (0403 resolved)

0403 now emits `nros.wcet.measurements/1`: per measurement `min`/`max`/`mean`,
`iterations` and the identity of what was measured, plus the conditions
`counter_valid`, `cpu`, `profile` and `commit`. Design against that.

Two properties of it bear directly on the questions above, and both should be
designed WITH rather than around:

* **The artifact states that it is not convertible to time.** The bench cannot
  read the part's clock, so `clock_hz` is null and `convertible_to_time` is
  false. That is the UNIT question, made concrete: this schema either carries a
  clock rate obtained elsewhere, or treats cycles as a first-class unit. What it
  must not do is let a consumer supply a plausible rate — a manufactured `ms` is
  the same failure as a manufactured zero, dressed better.
* **No run has produced an artifact yet.** QEMU cannot measure and there is no
  hardware lane, so the format is real and the numbers are not. A schema
  validated against the format is a genuine result; a claim that a measurement
  flowed end-to-end is not available yet. Keep the two apart when reporting.

### Designed 2026-08-18 — RFC-0078

The schema is [RFC-0078](../design/0078-wcet-is-declared-per-profile.md), which
answers all four questions above:

* **keyed on** a named measurement profile (`stm32f4-168mhz-release`). Board id
  and platform family both collapse into it — the first is a profile with one
  member, the second is a profile that omits the rate it needs anyway;
* **what converts**: NOT the measured maximum. Revised 2026-08-18 after
  checking the design against the WCET literature — the longest observed time is
  a high-water mark, not a bound ("this approach cannot provide any guarantees",
  Wilhelm et al.), so converting it would count an under-estimate as measured.
  Observation (`max_observed_cycles`) and bound (`bound_cycles`, or a declared
  `margin_percent`) are separate, and observation alone yields no `exec_ms`;
* **unit**: declare CYCLES plus the profile's `clock_hz`, and convert to `ms`
  inside this repo. rlm's slot is milliseconds by cross-repo agreement and
  cannot be redefined here; declaring `ms` directly would put the conversion
  where a human holds the rate in their head, which is where an invented rate
  enters;
* **granularity**: per BOUNDARY, at rlm's own `node/path` identity, so
  `ChainFeasibleWithoutWcet`'s `boundaries_without_wcet` and the declarations
  join by set difference. Primitive composition stays out of scope;
* **provenance**: the conditions 0403 already emits travel with the number.
  Automatic staleness invalidation is explicitly NOT solved — only that the
  commit is recorded so a reviewer can ask.

The invariant is preserved and stated as outranking every decision: absent stays
representable and stays the DEFAULT, and the success condition is
`ChainFeasibleWithoutWcet` naming fewer boundaries — never going quiet.

RFC only, by scope. The Rust type and its validator are deliberately a separate
work item so the design is reviewable before an implementation makes it
expensive to revert.

One correction the design turned up: the pinned rlm is **v0.1.6**
(rev `b9f45f1`), and CLAUDE.md still describes the dep as tag `v0.1.0`.

The KEYING question is settled by the above. What remains hard, and what this
issue's original text called out: the artifact
records `cpu`/`profile`/`commit` for the run that produced it, but says nothing
about which OTHER contexts that number may be applied to. That judgement is this
schema's to make.

The invariant to preserve, whatever the shape: **absent must remain
representable and must remain the default.** A schema that requires every
boundary to carry a WCET will get zeros written into it, and the tree will be
back where 0259 found it — with the added problem that the zeros are now
signed by a developer.

## Resolved (2026-08-21) — implemented in `7ccfd38c9`, never closed

The 2026-08-18 note above ends "RFC only, by scope. The Rust type and its
validator are deliberately a separate work item so the design is reviewable
before an implementation makes it expensive to revert." That work item landed
(`feat(phase-357 W1, #404)`) and the issue outlived it. Verified by RUNNING the
suite, not by reading the type.

### The four questions, answered in code

| question | answer |
| --- | --- |
| keyed on | `[wcet.profiles.<name>]` — a named measurement context; `[wcet.select]` maps deploy target -> profile |
| unit | `max_observed_cycles` + the profile's `clock_hz`, converted in-repo. `clock_hz` is `Option`: a profile without one is still valid and yields NO `exec_ms` |
| granularity | `BoundaryWcet` per boundary at rlm's `node/path` identity, so `boundaries_without_wcet` and the declarations join by set difference |
| provenance | `cpu`, `profile`, `measured_at_commit`, `counter_valid`, `source`, plus free-text `conditions` |

### The invariant holds, and the tests say so by name

"Absent must remain representable and must remain the default" is enforced
structurally, and each arm has a test that fails if it stops being true:

```
wcet::tests::no_clock_rate_yields_no_time_and_never_a_zero
wcet::tests::a_dead_counter_cannot_be_declared_as_measured
wcet::tests::a_bound_below_what_was_observed_is_rejected
wcet::tests::a_max_below_its_min_is_not_a_measurement
wcet::tests::a_declared_margin_turns_an_observation_into_a_bound
mapper_input::tests::an_observation_alone_reaches_nothing
mapper_input::tests::no_profile_means_no_exec_ms_anywhere
mapper_input::tests::a_declared_wcet_reaches_exec_ms_and_an_undeclared_one_stays_absent
```

`cargo test -p nros-orchestration-ir` — 92 passed, 0 failed. The end-to-end one
is the issue's original complaint inverted: `MapperPath.exec_ms` now has exactly
one way to become `Some(..)`, and it is a declaration a human wrote.

### "What remains hard" is answered too

This issue closed by saying the artifact "records `cpu`/`profile`/`commit` for
the run that produced it, but says nothing about which OTHER contexts that
number may be applied to. That judgement is this schema's to make."

`[wcet.select]` is that judgement, made explicitly: a project states which
profile a deploy target uses, so re-using a number across contexts is a written,
reviewable claim rather than an inference. And it cannot fail quietly —
`WcetSelectionError::UnknownProfile` is a HARD error, on the reasoning that "a
typo must not read as 'this board has no measurements'". That is the same
failure this issue and 0259 exist to prevent, caught one layer earlier.

### The correction in the 2026-08-18 note is also stale

It records that "the pinned rlm is v0.1.6 (rev `b9f45f1`), and CLAUDE.md still
describes the dep as tag `v0.1.0`". CLAUDE.md now says `v0.1.8` across every
nano-ros crate, and adds the standing instruction to read the pin from the
manifests rather than from that file.

### Downstream

Issue 0259 ("derived scheduling is quantitatively inert") loses its blocker:
`ChainFeasibleWithoutWcet` now has a remedy, and the success condition it names
— the warning listing FEWER boundaries rather than going quiet — is exactly what
the declaration path produces. Whether 0259 closes is its own question; this one
supplied what it was waiting for.
