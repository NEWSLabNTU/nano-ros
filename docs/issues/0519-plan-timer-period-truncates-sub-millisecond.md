---
id: 519
title: "The plan's timer period is still milliseconds, so `nros explain` shows a
  sub-millisecond timer as `0ms`"
status: open
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
