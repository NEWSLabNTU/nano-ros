---
id: 259
title: "RTOS realizer never derives placement / non_preempt_scope from the model — both dims hardcoded NotRequested"
status: open
type: limitation
area: orchestration
related: [phase-296]
---

## Finding (phase-296 W5.5–W5.11 placement/preempt consumer work, 2026-07-24)

`realize_rtos` (`packages/cli/nros-cli-core/src/orchestration/rtos_realizer.rs`)
derives the `deadline` and `budget` dims from the self-derived DAG facts
(`node_facts` → `max_latency_ms`, timer-rate period, exec/WCET) and lowers them
to `Native` / `Backfill` / `Degrade`. But the `placement` and `non_preempt_scope`
dims are HARDCODED to `NotRequested` (rtos_realizer.rs ~line 341):

```rust
// non_preempt_scope + placement: not derived from the model yet.
let preempt_real = DimRealization::NotRequested;
let placement_real = DimRealization::NotRequested;
```

So `RealizedNode.core` and `.preempt_threshold` are always `None` — the derived
schedule NEVER assigns a core pin or a preemption threshold.

## Why it matters

The board consumers for both dims now EXIST and are e2e-verified (phase-296
W5.7/W5.8 Zephyr core-pin, W5.9 NuttX sporadic, W5.10 ThreadX
preempt-threshold, W5.11 NuttX/FreeRTOS core-pin), but they only fire from the
EXPLICIT per-tier knobs in `system_model.yaml` (`<platform>.core`,
`<platform>.preempt_threshold`). The DERIVED-schedule path (RFC-0052's whole
point — `derive_execution_from_contracts` when a model declares no
`execution.tiers`) can never produce those knobs, so a self-derived schedule
silently omits placement + non-preemption.

## Blocker (design-open)

This is NOT a mechanical gap — `model.contracts.node_paths` has no fact that
implies "pin to a core" or "this callback is non-preemptible". Deriving them
needs a model vocabulary decision (RFC-0052 §Open questions: dims-on-segment vs
dims-on-callback; and what contract fact maps to placement/non-preemption).
Candidates: a criticality/isolation contract → core pin; a
mutual-exclusion / shared-resource contract → non_preempt_scope. Resolve in
RFC-0052 before implementing.

## Direction

1. RFC-0052: decide the contract vocabulary for placement + non_preempt_scope.
2. Extend `node_facts` / `MapperPath` to carry the new facts.
3. Fill `placement_real` / `preempt_real` in `realize_rtos` (Native where the
   `SchedCaps` support it, Degrade/Backfill otherwise, recorded like the other
   dims); set `RealizedNode.core` / `.preempt_threshold`.
4. Emit the knobs from `derive_execution_from_contracts` into the synthesized
   `[tiers.*]` rows so the existing consumers fire on the derived path.
