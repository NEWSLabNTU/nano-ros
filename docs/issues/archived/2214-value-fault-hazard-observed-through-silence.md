---
id: 2214
title: "A value-fault hazard is lowered onto its guard's silence row, so the image detects it whenever the guard goes quiet"
status: resolved
type: bug
area: [orchestration, codegen]
found: 2026-11-02
related: [phase-462, issue-2173]
resolved_in: "PR #3013"
---

## The gap

`contract_monitors::hazard_rows` lowered every hazard onto the rows that
watch its guard topics, whatever the hazard's `on:`. Those rows (an age
row's silence lease, a rate row's short window) are omission evidence. A
hazard on a REPORTED fault (`on: reported` with a `when:` predicate: the
stream keeps arriving and its value says the fault) was therefore observed
through the silence row: DETECTED whenever the stream went quiet, which is
another hazard's fault, and never when its own value came. The safety
island's `odd_exit` (`on: reported`, `when: { field: autonomous, equals:
false }`) read `hazard-detected` beside `hpc_loss` on every silence, and its
ladder's top rung was armed and judged (`hazard-route-runtime`) against a
fault it does not cover. `ftti_budget` already called such a hazard's
classes unobserved on the image; the bake disagreed with it.

## Resolution

`hazard_rows` lowers a hazard only when its `on:` is empty (every class) or
names `omission` or `loss`. `reported` alone, or `late`, is not lowered: no
row the image bakes can see it. Test:
`a_reported_fault_is_not_lowered_onto_the_silence_row`.
