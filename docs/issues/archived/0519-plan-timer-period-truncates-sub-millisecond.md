---
id: 519
title: "The plan's timer period is still milliseconds, so `nros explain` shows a
  sub-millisecond timer as `0ms`"
status: resolved
type: bug
area: orchestration
related: [issue-0505, issue-0518]
---

## What is wrong

`#505` gave the runtime microsecond timer resolution and added `period_us` to
the metadata JSON alongside the kept `period_ms`. The CLI's plan path was not
part of that change and still reads only the truncating field:

```rust
// orchestration/planner.rs:1463
"period_ms": entity.get("period_ms").and_then(Value::as_u64).unwrap_or(0),
```

`EntityMetadata` emits `period_ms: entity.period_ms.unwrap_or(0)`, so a timer
declared at 500 µs arrives as `period_ms: 0` while `period_us: 500` sits
unread beside it. The plan records `0`, and `nros explain` renders:

```
timer  0ms
```

## Severity: display and artifact fidelity, not runtime behaviour

`PlanEntity::Timer.period_ms` has exactly one consumer — `cmd/explain.rs:250`.
It does **not** reach generated code: no emitter under `codegen/entry/` reads
it (the C/C++ emitters' `period_ms` hits are all the unrelated
`publish_period_ms` *parameter* in test fixtures). So a sub-millisecond timer
still runs at the right rate; it is reported at the wrong one, and the
committed plan JSON records the wrong one.

That is worth fixing anyway: "the plan says 0ms and the thing fires at 500 µs"
is the kind of disagreement that costs an hour the first time someone trusts
the plan.

## Why it was not fixed alongside 0518

0518 was the parse failure — `SourceTimer` is `deny_unknown_fields` and
rejected #505's *added* field. That fix is additive and touches one struct.

This one changes the **plan schema**, which ripples: `PlanEntity::Timer`,
`explain.rs`, and nine committed `tests/fixtures/orchestration/plan_*.json`.
It also needs a decision that is #505's to make rather than mine — whether
`SchedContext.period_ms` / `budget_ms` / `deadline_ms` move to microseconds at
the same time. They have the same truncation and the same "end to end" claim
over them, and migrating the timer alone would leave the plan carrying two
different time units.

## Suggested shape

Prefer `period_us` where present and fall back, mirroring what #505 did for
the runtime ("runtime prefers `_us` and falls back"), rather than replacing
the field — the hand-written fixtures predating #505 carry only `period_ms`
and are legitimate inputs.

## Resolved 2026-08-16 (phase-357 W3) — the render was already fixed; nothing pinned it

The timer half of this issue is fixed in the tree, and was before I looked:

* `planner.rs` emits `period_us` beside `period_ms` (lines 1468, 2845), derived
  through `source_metadata::timer_period_us_from_json`;
* `PlanEntity::Timer` carries `period_us: Option<u64>`, optional so a pre-#505
  plan still deserializes;
* `explain.rs` renders exactly the shape this issue suggested —

```rust
let us = period_us.unwrap_or_else(|| period_ms.saturating_mul(1_000));
if us % 1_000 == 0 { format!("timer  {}ms", us / 1_000) } else { format!("timer  {us}us") }
```

So a 500 µs timer prints `timer 500us`, not `timer 0ms`.

**What was missing is a test.** No case anywhere pinned the microsecond render,
so the exact defect this issue is about could return silently — and the fix is
one `unwrap_or_else` away from being undone by anyone tidying the match arm.
Three added in `cmd/explain.rs`:

| test | pins |
| --- | --- |
| `a_sub_millisecond_timer_is_not_reported_as_zero` | the defect itself: 500 µs must not render `0ms` |
| `a_whole_millisecond_timer_still_prints_milliseconds` | the common case — a fix that printed `10000us` everywhere would pass the first test and be a regression |
| `a_pre_505_plan_without_period_us_widens_the_millisecond_field` | absent `period_us` means "widened", not zero; the hand-written pre-#505 fixtures are legitimate inputs |

Verified by sabotage: restoring `let us = period_ms.saturating_mul(1_000)` fails
the first test and ONLY the first, which is what makes the other two worth
having.

## Still open, and deliberately NOT taken here: the SchedContext half

This issue also flagged that `SchedContext.period_ms` / `budget_ms` /
`deadline_ms` carry the same truncation, and said the decision was #505's to
make rather than this issue's. That is still true of the code —
`explain.rs:151-156` prints `period={v}ms`, `budget={v}ms` — and #505 is now
resolved without having moved them.

So the question is now unowned rather than deferred. It is a plan-SCHEMA change
(the issue's own reason for splitting it from the timer fix) and belongs with
phase-357's W1 schema work, where the unit for declared timing is settled once
rather than per-field.
