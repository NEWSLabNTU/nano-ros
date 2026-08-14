# Phase 357 — WCET as declared data: making derived scheduling mean something

**Status (2026-08-15). PLANNING — nothing implemented.** Three orchestration
issues that are one dependency chain, not three tasks.

**Owns:** [issue 0259](../issues/0259-realizer-placement-nonpreempt-not-derived.md),
[issue 0404](../issues/0404-wcet-declaration-schema.md),
[issue 0519](../issues/0519-plan-timer-period-truncates-sub-millisecond.md).

**Related:** [phase-296](phase-296-system-model-consumption.md) (IN PROGRESS,
names #259 and #260), [phase-162](phase-162-rt-scheduling-harness.md) (the
harness that can measure), [phase-356](phase-356-test-evidence-and-measurement-trust.md)
W2 (the bench that produces the numbers), RFC-0031 / the `system.toml` schema.

## The chain, in order

1. **#404** — there is no schema for declaring a measured WCET: where it lives,
   what it is keyed on, what makes it trustworthy.
2. **#259** — because there is no WCET in the model, derived scheduling is
   **quantitatively inert**: blocking is unmodelled, and `budget`, `placement`
   and `non_preempt` cannot be derived.
3. **#519** — separately, the plan's timer period is still milliseconds, so
   `nros explain` renders a sub-millisecond timer as `0ms`.

#259 cannot be fixed before #404, because the thing it lacks is the data #404
defines. Attempting #259 first produces a second, undeclared place for WCET to
live — the "second spelling" failure CLAUDE.md names repeatedly.

#519 is independent and small; it is here because it is the same subsystem and
because a model that gains WCET while still truncating periods to `0ms` would be
newly misleading.

---

## W1 — A schema for a declared WCET (#404)

The question is not "add a number to `system.toml`". It is the three the issue
asks:

* **Where it lives.** `system.toml` is authored; SystemModels are BUILD
  ARTIFACTS and never committed (phase-330 W4.a/W7, gate
  `check-no-tracked-models`). A measured WCET is neither purely authored nor
  purely derived, which is the actual design problem.
* **What it is keyed on.** A WCET is only meaningful for a (callback, platform,
  board, toolchain, optimisation level) tuple. Under-keying it is how a number
  measured on a host gets applied to a Cortex-M.
* **What makes it trustworthy.** How was it measured, when, on what, and what
  invalidates it. Phase-356 W2 is the measurement side: a bench that reports
  zeros from a dead counter must not be able to feed this.

Design decision ⇒ this wants an **RFC**, not just a phase work item. Write it
before implementing.

**Acceptance.** An RFC in `docs/design/` that answers all three questions, and a
worked example carrying one real measured callback end-to-end.

## W2 — Make derived scheduling quantitative (#259)

Blocked on W1. Once WCET is declarable and keyed, the derivations the issue
names — blocking analysis, and deriving `budget` / `placement` / `non_preempt`
— become expressible.

The issue's word is "inert", not "wrong": today the scheduling model produces
*something*, and that something is not informed by timing. Any work here should
say plainly what changes in `nros explain` output, because that is where a user
sees whether the model has become quantitative.

**Acceptance.** For a system with declared WCETs, at least one of `budget`,
`placement` or `non_preempt` is DERIVED rather than authored, and `nros explain`
shows the derivation's inputs. Do not close #259 on a schema alone.

## W3 — The plan's timer period is milliseconds (#519)

`nros explain` shows a sub-millisecond timer as `0ms`. Independent of W1/W2 and
fixable now.

Note phase-352 (COMPLETE) already moved the platform clock ABI to nanoseconds
with an expressible resolution — "one nanosecond symbol, plus the resolution
nobody could ask for". So the runtime side of this unit question is settled and
the plan should follow it rather than invent a third unit.

**Acceptance.** A sub-millisecond timer renders with its actual period.
Whatever unit is chosen matches phase-352's, and the choice is stated.

---

## Deliberately not doing

* **Not measuring WCET on hardware in this phase.** That is phase-162's harness
  and phase-356 W2's bench. This phase defines what a measurement must look like
  to be usable, and consumes it.
* **Not deriving anything before the schema exists.** Explicitly the failure
  mode this ordering avoids.
