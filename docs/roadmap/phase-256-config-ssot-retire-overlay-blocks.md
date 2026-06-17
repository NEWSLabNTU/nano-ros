# Phase 256 — Config SSoT endgame: retire the remaining `nros.toml` overlay blocks

Status: **Design (2026-06-17)** · Implements [RFC-0004 §3.1](../design/0004-configuration-and-transports.md)
SSoT-per-concern · Follows [phase-254](phase-254-config-ssot-unify-codegen-paths.md) (capabilities)
+ [phase-255](phase-255-rmw-config-unify.md) (RMW) · Closes the bulk of
[issue 0076 §A](../issues/0076-followups-config-ssot-and-safety-e2e-arc.md).

## Why — the same duality, five more times

phase-254 (capabilities) and phase-255 (RMW) each fixed ONE instance of a structural
duality: a concern declared in **two decoupled places** read by **two codegen paths** —

| Path | Reads | From |
| --- | --- | --- |
| Rust build plan | `planner.rs` `collect_*` / `schema_build_json` | per-package **`nros.toml`** overlay |
| C/C++ bake | `codegen_system.rs` (`render_system_config_h`, tier/shared-state resolution) | typed **`system.toml`** |

Every remaining config concern still has this split. The overlay path is the
**action-at-a-distance hazard** RFC-0004 §3.1 forbids (a value in some package's `nros.toml`
silently changes the system build). This phase migrates the rest to typed `system.toml`,
makes the overlay a deprecated warn-fallback, then removes it — after which `nros.toml`
reverts to its RFC-0004 §5 embedded-runtime-only role and the same-name collision is gone.

## The five remaining blocks (mapped)

| Block | Overlay reader (`planner.rs`) | Typed `system.toml` field today | Plan output | Decision |
| --- | --- | --- | --- | --- |
| `[build]` rest (`target`/`board`/`profile`/`features`/`cfg`/`optimize`/`cargo`/`cc`/`[[transport]]`) | `schema_build_json` (planner.rs:819) | none (only `rmw` unified, ph-255) | `PlanBuildOptions` (plan.rs:689) | → `[deploy.<t>]` (per-target build tuning) |
| `[lifecycle]` | `collect_lifecycle` (planner.rs:1162) | **none** | `Option<PlanLifecycle>` (plan.rs:34) | → new typed `[lifecycle]` |
| `[param_persistence]` | `collect_param_persistence` (planner.rs:1181) | **none** | `Option<PlanParamPersistence>` (plan.rs:45) | → new typed `[param_persistence]` |
| `[[scheduling.contexts]]` | `collect_sched_contexts` (planner.rs:1128) | `[tiers.*]` `TierDef` (schema:434) — **parallel model** | `Vec<PlanSchedContext>` (plan.rs:17) | `[tiers]` is SSoT; derive contexts from it |
| `[[shared_state]]` | `collect_shared_state` (planner.rs:1244) | `shared_state: Vec<SharedStateDecl>` (schema:441) — **richer model** | `Vec<PlanSharedRegion>` (plan.rs:40) | typed `SharedStateDecl` is SSoT |

**Two clean adds** (`lifecycle`, `param_persistence`: no existing field) and **three
reconciliations** (`build` rest: pick a home; `scheduling` + `shared_state`: a typed model
already exists in `system.toml` for the bake but the planner ignores it).

## Design decisions

1. **`[build]` rest → `[deploy.<t>]`.** `target`/`board` already live in `DeployTarget`;
   `profile`/`optimize`/`cargo`/`cc`/`features` are per-target build tuning, so they belong in
   the same per-deploy block. `[[transport]]` is topology → it joins `[[domain]]`/`[[bridge]]`
   at `system.toml` top level (already the home for transports per RFC-0004 §6). The planner
   resolves the build shape for the selected target through the same `--target` key phase-255
   used, so a multi-target system gets per-target build tuning with no overlay.

2. **`[tiers]` (RFC-0015) is the scheduling SSoT; `[[scheduling.contexts]]` retires.** The bake
   already resolves `[tiers.*]` + `[[node_overrides]]` into per-RTOS task knobs
   (`resolve_tiers`, `ResolvedTierTable`). The planner's overlay `scheduling.contexts`
   (executor/priority/period) is the **pre-RFC-0015** model. phase-256 makes the planner derive
   `PlanSchedContext` from the resolved tier table (the same input the bake consumes), so both
   paths schedule identically. The overlay becomes a warn-fallback.
   *(Reconciliation note: tier knobs (symbolic priority/scheduling class) and the older context
   fields (period_ms/budget_ms/deadline) are not 1:1 — Wave 4 must map the tier model onto
   `PlanSchedContext`, extending `TierDef` with the timing fields the context model carried if
   the runtime still needs them, or dropping unused ones. Confirm against RFC-0015 §4.2.)*

3. **Typed `SharedStateDecl` is the shared-state SSoT; raw `{id,bytes}` overlay retires.** The
   bake consumes `SharedStateDecl` (name/schema/storage/sync — schema-driven size). The planner's
   overlay `{id,bytes}` is the raw pre-typed model. phase-256 lowers `SharedStateDecl` →
   `PlanSharedRegion` (size computed from the declared schema/fields, the bake's existing
   sizing), so the planner stops reading the raw overlay.

4. **Per-value provenance is the enabling primitive** (for `nros config show` + `nros check`).
   Today `load_toml_values` (params.rs:48) returns `Vec<Value>` — file attribution is lost at
   merge. phase-256 threads `(PathBuf, Value)` through overlay load so each resolved value knows
   its source file. This is what makes the deprecation warnings name the offending file and
   powers `config show`'s provenance column.

## Waves

- **Wave 0 — provenance primitive.** `load_toml_values` → returns source-tagged values
  (`Vec<(PathBuf, Value)>` or a `SourcedValue` wrapper); `schema_build_json` / each `collect_*`
  records which file each value came from. No behaviour change — sets up Waves 1-5's warnings +
  the two infra commands. Unit-test the tagging.
- **Wave 1 — `[lifecycle]` → typed.** Add `lifecycle: Option<SystemLifecycle>` to `SystemToml`;
  `collect_lifecycle` prefers it, warns on overlay (the phase-254 pattern). Test parse +
  precedence.
- **Wave 2 — `[param_persistence]` → typed.** Same shape: `param_persistence:
  Option<SystemParamPersistence>` on `SystemToml`; `collect_param_persistence` prefers it.
- **Wave 3 — `[build]` rest → `[deploy.<t>]`.** Extend `DeployTarget` with `profile`/`optimize`/
  `cargo`/`cc`/`features`; move `[[transport]]` to a top-level typed `system.toml` table.
  `schema_build_json` resolves the build shape from the selected deploy target, preferring it
  over the `[build]` overlay (warns). The single biggest fixture-migration surface — sweep
  `nros.toml` `[build]`/`[[transport]]` users.
- **Wave 4 — `[[scheduling.contexts]]` → `[tiers]`.** Planner derives `PlanSchedContext` from
  `ResolvedTierTable` (the bake's input); overlay `scheduling.contexts` becomes a warn-fallback.
  Resolve the tier-vs-context field mapping (decision 2). Highest design risk — do it after the
  mechanical waves.
- **Wave 5 — `[[shared_state]]` → typed `SharedStateDecl`.** Planner lowers `SharedStateDecl` →
  `PlanSharedRegion` (schema-driven size); stop reading the raw `{id,bytes}` overlay.
- **Wave 6 — `nros config show`.** New-model command: print the **resolved effective config**
  for a system + **per-value provenance** (which file each value came from), using Wave 0's
  tagging. Replaces the retired pre-212 `config.toml` reader (`cmd/config.rs`).
- **Wave 7 — `nros check` legacy-overlay flag.** `nros check` flags any value still sourced from
  a per-package `nros.toml` overlay (Wave 0 provenance) + prints the removal date — the
  action-at-a-distance guard. Extends `check`'s current plan/schema validation (`check_plan_file`,
  `collect_plan_warnings`).
- **Wave 8 — deploy-metadata precedence (leakage).** Make the `[package.metadata.nros.deploy.<t>]`
  + `[workspace.metadata.nros]` Cargo-native projection **explicit and non-silent**: when a
  `system.toml` exists for the same scope it is authoritative (RFC-0004 §3.1 ladder: flag >
  `system.toml` > native projection > default), surfaced by `config show`, not an overlay merge.
- **Wave 9 — migrate fixtures + docs + retire.** Move every remaining `nros.toml` overlay block
  to `system.toml`; RFC-0004 §4 records each typed field; remove the overlay readers after the
  release (warn-fallbacks become hard errors), collapsing the `nros.toml` same-name collision.

## Acceptance

- Each of the five blocks declared **once** in `system.toml` (typed); both the planner and the
  bake resolve it from there. The per-package `nros.toml` overlay is a warn-fallback, then gone.
- `nros config show <system>` prints the resolved config with a provenance column (source file
  per value).
- `nros check` warns on any overlay-sourced value with a removal date.
- Generated output byte-identical for a system whose overlay values already equal the resolved
  `system.toml` values (the migration is value-preserving).
- `nros.toml` carries only the RFC-0004 §5 embedded direct-mode runtime sections.

## Risks / decisions

- **Wave 4 (scheduling) is the design crux.** The tier model (RFC-0015) and the context model
  (period/budget/deadline) are not 1:1. If the runtime still consumes the timing fields, extend
  `TierDef`; if they were vestigial, drop them. Pin this against RFC-0015 §4.2 + the executor's
  actual scheduling inputs before coding.
- **Wave 3 transport home.** `[[transport]]` could stay a `[deploy.<t>]` sub-array or be a
  top-level `system.toml` table. RFC-0004 §6 already documents transports at top level →
  top-level is the consistent choice; per-deploy transport sets are a `deploy.<t>.transports`
  follow-up if needed.
- **Scope.** Large. The mechanical waves (0-2, 5) are low-risk and land first; Waves 3-4 carry
  the design weight; 6-8 are the audit surface; 9 is the cleanup. Each wave is independently
  landable and value-positive (one more concern leaves the overlay).
