# Phase 357 — WCET as declared data: making derived scheduling mean something

**Status (2026-08-16). W3 DONE; W1 BLOCKED, W2 blocked behind it — nothing in
this phase is actionable until [phase-356](phase-356-test-evidence-and-measurement-trust.md)
W2 emits a machine-readable WCET artifact (#403).** Three orchestration
issues that are one dependency chain, not three tasks.

* **W1 (#404, the WCET schema)** — **BLOCKED on #403**, which this phase failed
  to record when it was opened. #404's own Direction section is explicit:
  "Nothing here should be designed before 0403 produces an artifact … Doing it
  in the other order would produce a schema shaped around a hypothetical
  measurement, which is how the keying question gets answered by guess."
  So W1 is not merely "wants an RFC first" — it cannot be written yet.
* **W2 (#259, quantitative scheduling)** — blocked on W1, by construction.
* **W3 (#519, sub-millisecond timer period)** — DONE. The render was already
  correct; what was missing was a test pinning it, now added and proven by
  sabotage. The issue's SchedContext half is unowned and folded into W1 below.

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

**BLOCKED, and the blocker is load-bearing rather than procedural.** #404 says
not to design this before #403 emits an artifact, because a schema written
against a hypothetical measurement answers the keying question by guess — and
keying is the question that decides whether a number measured on an STM32F407 at
168 MHz can be applied to a Cortex-M3 in QEMU.

The "worked example carrying one real measured callback" in the acceptance above
cannot exist until a producer emits one. That is phase-356 W2 (#403), whose
first item landed 2026-08-16: the bench now REFUSES to emit zeros from a run
that could not measure, so whatever it eventually produces will not be a table
of the most optimistic value a WCET can take. The remaining half — a
machine-readable artifact instead of prose — is what unblocks this.

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

**DONE 2026-08-16.** The render already preferred `period_us` and fell back to a
widened `period_ms`; a 500 µs timer prints `timer 500us`. Nothing pinned it, so
three tests were added in `cmd/explain.rs` and verified by sabotage — restoring
the truncation fails the 0519 case and only that one.

**Folded into W1:** #519 also flagged `SchedContext.period_ms` / `budget_ms` /
`deadline_ms`, deferring the unit decision to #505. #505 has since resolved
WITHOUT moving them, so that question is unowned rather than deferred. It is a
plan-SCHEMA change and belongs where the unit for declared timing is settled
once, which is W1 — not three more per-field migrations.

---

## Deliberately not doing

* **Not measuring WCET on hardware in this phase.** That is phase-162's harness
  and phase-356 W2's bench. This phase defines what a measurement must look like
  to be usable, and consumes it.
* **Not deriving anything before the schema exists.** Explicitly the failure
  mode this ordering avoids.
