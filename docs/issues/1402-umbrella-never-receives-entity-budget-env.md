---
id: 1402
title: "The synthesised `nros_ws_runtime` umbrella carries two declared facts
  and none of the entity BUDGET knobs `_nros_entity_budget_env` exists to
  deliver — measured, mechanism not established"
status: open
type: bug
area: [build, cmake]
severity: medium
found: 2026-09-21
related: [1390, 1388, 0460, 0196, phase-392]
---

## What was measured

Issue 1390 instrumented `nros-node/build.rs` to record, per unit, the knobs it
resolved. Across one serial `just threadx_linux build-examples` (18 `nros-node`
compilations), the `nros_ws_runtime` umbrella of `examples/workspaces/mixed`
resolved the UNNARROWED executor sizing — `cbs=4 sc=8 arena=74240` — and its
environment carried only:

```
NROS_DECLARED_NODES=3
NROS_DECLARED_INFRA_QUERYABLES=none
```

No `NROS_DECLARED_EXECUTOR_MAX_CBS`, no `NROS_ENTITY_COUNT_*`, none of the
budget family.

## Why that is surprising

`cmake/NanoRosEntityFacts.cmake` exists to deliver exactly those, and its own
docstring says the delivery was widened to EVERY corrosion target precisely
because the umbrella is one of two cargo roots:

> WHY EVERY CORROSION TARGET AND NOT JUST THE UMBRELLA (phase-392 W5.g
> follow-up). `zpico-sys` is compiled once per CARGO ROOT, and a workspace has
> two: the synthesised umbrella (`nros_ws_runtime`) and the repo root … Measured
> on `mixed`: 6 `zpico-sys` units, and only the 1 under the umbrella could ever
> see the env.

`nros_entity_facts_env` computes `_nros_entity_budget_env` before the
queryable-table early return, so the intent is clearly that the umbrella gets
the budget. The measurement says it does not.

## What is NOT established

**The mechanism.** Candidates, none confirmed: the deferred flush does not queue
this target; a guard inside `_nros_entity_budget_env` returns empty for a
workspace with no single image; the env is applied to a different corrosion
target name than the one that compiles `nros-node`; or the accumulator is empty
at the point the umbrella's env is computed (the failure phase-392 W5.g fixed
once already, for a different consumer).

Nor is it established that delivering the budget would CHANGE anything here.
Issue 1390 argued it would not change that issue's decision: the umbrella serves
every entry in the workspace, so the most it could take is the configure-wide
MAX, which is still not any one image's narrowing. That argument is about the
executor backing specifically and does not cover the rest of the budget family.

## Why it is worth an issue anyway

A declared delivery mechanism that does not deliver is the tree's most common
defect shape, and this one has a docstring asserting the opposite of the
measurement. Either the env is arriving by a route the instrumentation missed —
in which case the measurement needs correcting — or a knob family silently
defaults on a unit the codebase believes it narrows.

## Direction

Reproduce first: instrument or log the umbrella target's `corrosion_set_env_vars`
argument list at configure time, and compare against what
`_nros_entity_budget_env` returned in the same configure. Then either fix the
delivery or correct the docstring's claim — but not by reasoning about which is
true.

Found while resolving issue 1390; reported there under "left undone" and filed
here so it is not lost.
