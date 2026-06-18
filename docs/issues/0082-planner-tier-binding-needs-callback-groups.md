---
id: 82
title: Planner can't lower tiers to PlanSchedContext — lacks node callback_groups (W4.2 blocker)
status: open
type: design
area: orchestration
related: [phase-256, rfc-0015, issue-0076]
---

## Blocker (2026-06-18)

phase-256 **W4** (decision A: tiers are the single scheduling home) needs the
**planner** to derive `PlanSchedContext` from the resolved tier table. W4.1 landed
the `TierDef`/`ResolvedTier` EDF fields. **W4.2 is blocked.**

`resolve_tiers` requires the node-declared `callback_groups` (the `group → tier`
binding), which live in Cargo `[package.metadata.nros.node].callback_groups` →
`NrosConfig`. **Only the bake (`codegen_system`) loads `NrosConfig`** (via
`collect_callback_groups`). The planner:

- builds `Workspace::discover`, **not** `NrosConfig`;
- works from prebuilt **source-metadata JSON**, which carries **no** `callback_groups`
  (it has per-callback `group`, but not the `group → tier` map);
- deliberately **does not shell `cargo metadata`** — it consumes prebuilt artifacts.

So the planner can see a callback's `group` but cannot map it to a tier.

## Options

- **(a) Source-metadata route (preferred).** The codegen/build stage emits the
  node's `callback_groups` into the per-node source-metadata JSON the planner
  already reads. Keeps the planner cargo-free; the binding rides the existing
  artifact. Touches the metadata emitter + the source-metadata schema.
- **(b) `NrosConfig` in the planner.** `plan_system` loads `NrosConfig::from_workspace`
  → `collect_callback_groups`. Simple, but introduces a `cargo metadata` shell in
  the planner — against its artifact-consuming design.
- **(c) Unify Rust scheduling through the bake.** The tier table feeds both the C
  bake AND the Rust runtime codegen, so the planner stops owning sched contexts.
  Bigger refactor; removes the duality at the root.

## Decision — (c), locked 2026-06-18

Resolve tiers in the **codegen tools** (`generate` for Rust, the bake for C),
**not** the planner. The planner stops owning scheduling. Chosen over (a) for
minimal change + keeping the planner simple; the drift risk (two resolution sites)
is mitigated by both tools calling the **same** `resolve_tiers` (already shared in
`nros-orchestration-ir`) + a shared `ResolvedTier → PlanSchedContext` mapping.

UX/maintainability trade accepted: tier errors surface at the codegen stage (per
language) rather than once at `nros plan`; consistency relies on both tools feeding
`resolve_tiers` the same inputs (target RTOS + `NrosConfig` callback_groups).

## Implementation plan (c)

1. **Shared mapping** — `ResolvedTier → PlanSchedContext` helper (priority `i64→u8`,
   `period_us`/`budget_us`/`deadline_us` carry, `class`/`deadline_policy` string →
   enum). Lives where both codegen tools reach it.
2. **`generate_package`** — load `NrosConfig` via `component_workspace`,
   `collect_callback_groups`, `resolve_tiers` → map → `SchedContextSpec`; bind
   callbacks by `(node, group)` → tier; replaces the plan's overlay `sched_contexts`.
3. **Bake** — already resolves tiers; align it to the shared mapping where it lowers
   to C (no behaviour change for the existing tier path).
4. **Planner** — drop the overlay `sched_contexts` emission; `plan.sched_contexts`
   goes vestigial (kept empty for transition, removed with the overlay in W9).

## State

- W4.1 (TierDef/ResolvedTier EDF fields) is landed.
- Decision (c) locked; implementation per the plan above. The overlay
  `[[scheduling.contexts]]` (0 users) retires with the `nros.toml` file (W9).
