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

## State

- W4.1 (TierDef/ResolvedTier EDF fields) is landed and independent.
- W4.2 waits on this decision. The overlay `[[scheduling.contexts]]` (0 users)
  stays as-is until W4.2 lands; it retires with the `nros.toml` file (W9) regardless.
