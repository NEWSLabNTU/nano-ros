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

## Design (explored 2026-06-18) — direct, lossless mapping

The integration was mapped against the real code. Findings:

- **`generate_package` has the inputs.** `GenerateOptions.component_workspace` →
  `NrosConfig::from_workspace` gives the bringup `[tiers]`/`[[node_overrides]]`/
  `[[component]]` + each component pkg's `callback_groups` — the *same* inputs the
  bake feeds `resolve_tiers`. So generate can resolve tiers itself.
- **`resolve_tiers` is reused as-is** (shared in `nros-orchestration-ir`); both bake
  and generate feed it the same `(tiers, node_overrides, component_names,
  callback_groups, target_rtos)` → drift bounded to one shared resolver.
- **Binding is feasible.** `collect_callback_groups` keys by the system
  `[[component]].name` (the instance name), so `ResolvedTier.members` are
  `(component-instance-name, group)`. A plan callback carries `group`, and its
  `PlanInstance` carries `component` → generate matches `(instance.component,
  callback.group) → tier`. (Confirm `PlanInstance.component == [[component]].name`
  in code.)

**Mapping decision — DIRECT `ResolvedTier → SchedContextSpec`, not via
`PlanSchedContext`.** `ResolvedTier` is **µs-native**, `SchedContextSpec` (runtime)
is **µs**, but `PlanSchedContext` is **ms** — routing through it round-trips
µs→ms→µs (**loses sub-ms**), forces `priority: i64 → Option<u8>`, and double
enum-maps (`SchedClass`≠`SchedClassSpec`, `DeadlinePolicy`≠`DeadlinePolicySpec`).
A **direct** tier→`SchedContextSpec` emitter is µs-lossless, clamps `i64`→`os_pri:
u8` once, and enum-maps straight to the runtime spec. The `PlanSchedContext` ms
units are an overlay-model legacy being dropped — don't re-inherit them.

- `priority: i64 → os_pri: u8` — clamp `[0,255]` + warn out-of-range. Zephyr
  negative coop priorities are the **bake/C** concern, not generate's Rust path.
- `class` string → `SchedClassSpec`, `deadline_policy` string → `DeadlinePolicySpec`
  — validated, clear error on unknown (the W4.1 doc lists the real variants).

## Implementation plan (c, direct mapping)

1. **`generate_package`** — load `NrosConfig` (component_workspace) → bringup tiers +
   `collect_callback_groups` → `resolve_tiers` → `ResolvedTierTable`.
2. **Direct `ResolvedTier → SchedContextSpec` emitter** (µs-native) + the
   `(component, group) → tier` callback binding; replaces the `plan.sched_contexts`
   read. `PlanSchedContext` / the planner sched path go vestigial.
3. **Bake** — unchanged (already resolves tiers from the same inputs).
4. **Planner** — drop the overlay `sched_contexts` emission; `plan.sched_contexts`
   retires with the `nros.toml` overlay (W9).

## Implementation map (W4.2b, explored 2026-06-18)

- **Precompute, render stays infallible.** `render_generated_tables(plan) -> String`
  (infallible, 3 callers incl. 2 tests). The tier renderer is fallible (validates
  class/policy), so resolve + render + bind in `generate_package` (already `Result`),
  store on `plan.build`, render just reads it — no fallibility ripple / test churn.
- **Threading:** `#[serde(skip)] pub tier_sched: Option<TierSched>` on
  `PlanBuildOptions` (like `workspace_root`); `TierSched { contexts: Vec<String>,
  bindings: Vec<(usize, usize)> }` (rendered context literals + callback→ctx-index).
- **Gate:** tier path only when `bringup.system.tiers` is non-empty; else the
  existing `plan.sched_contexts` path stays byte-identical.
- **Binding:** mirror `collect_callback_bindings` — iterate `plan.instances →
  callbacks` (callback_index++), look up the tier by `(instance.component,
  callback.group)` in the inverted `ResolvedTier.members`, emit `tier_index + 1`
  (slot 0 = default). `PlanInstance.component == [[component]].name` (planner-validated).

Change list: (1) `plan.rs` `TierSched` + skip field; (2) `generate_package` resolve
+ precompute → `plan.build.tier_sched`; (3) `render_generated_tables` reads it when
`Some`; (4) drop the W4.2a `#[allow(dead_code)]`.

## State

- W4.1 (EDF fields) + W4.2a (shared helpers + direct renderer) landed.
- W4.2b ready to implement per the map. The overlay `[[scheduling.contexts]]`
  (0 users) retires with `nros.toml` (W9).
