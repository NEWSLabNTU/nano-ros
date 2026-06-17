# Phase 256 — Config taxonomy tidy: retire the legacy config files, ground the option classes

Status: **Design (2026-06-17, re-scoped 2026-06-17)** · Implements
[RFC-0004 §3.1 + §9 tidy](../design/0004-configuration-and-transports.md) SSoT-per-concern ·
Follows [phase-254](phase-254-config-ssot-unify-codegen-paths.md) (capabilities)
+ [phase-255](phase-255-rmw-config-unify.md) (RMW) · Closes the bulk of
[issue 0076 §A](../issues/0076-followups-config-ssot-and-safety-e2e-arc.md).

> **RE-SCOPED 2026-06-17 (grounded sweep).** Originally "retire the remaining `nros.toml`
> overlay blocks." A sweep of `examples/**` found **0 `nros.toml` and 0 nano-ros `config.toml`
> files** — both legacy config files are fully unused, and `nros.toml`'s intended §5
> embedded-runtime role **never landed** (embedded net/RT lives in
> `[package.metadata.nros.deploy.<t>]` → `DeployOverlay` + board features + Kconfig). So the
> scope widens from "shrink `nros.toml` to its §5 role" to **"retire the `nros.toml` file
> entirely + the `config.toml` reader, and ground the live option taxonomy"** (RFC-0004 rewritten
> to match). The **live taxonomy is four surfaces**: `[package.metadata.nros.*]` / `nano_ros_*`
> (node + deploy), `system.toml` (multi-node system), `package.xml`, launch XML — plus Kconfig for
> the embedded build. **Scope classes:** node / system (agnostic) / deploy (per-target, incl. net
> + rmw/domain/locator overrides) / build+capability (lowered, not authored). The overlay-block
> migration (W1/W2) still happens — it's the mechanism for emptying `nros.toml` before deletion.

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
makes the overlay a deprecated warn-fallback, then removes it. **Per the re-scope above, after
emptying the overlay there is no surviving `nros.toml` role to preserve** — the file is removed
outright (the §5 embedded-runtime job was taken over by deploy metadata), and the `config.toml`
reader is scrubbed alongside.

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

- **Wave 0 — provenance primitive — DONE (2026-06-17).** Added `SourcedToml { path, value }` +
  `load_sourced_toml_values` (parse keeping file attribution) + `last_block_source(sourced,
  block)` (the file that last declared a top-level block — last-wins, matching the overlay merge)
  in `params.rs`. `load_toml_values` is now the path-dropping projection of the sourced loader, so
  every existing `&[Value]` consumer is untouched (no behaviour change, no wide re-typing). This
  is the primitive Waves 1-5 use to NAME the offending file in deprecation warnings and Waves 6-7
  use for `config show` provenance / `check`'s legacy flag. Test:
  `sourced_toml_tracks_provenance_per_block`. cli suite green.
- **Wave 1 — `[lifecycle]` → typed — DONE (2026-06-17).** Added `SystemLifecycle { autostart }`
  + `lifecycle: Option<SystemLifecycle>` on `SystemToml`. `schema_plan_json` now prefers the typed
  `[lifecycle]` (reusing the shared `system_caps` parse), falling back to `collect_lifecycle` (the
  `nros.toml` overlay) with a deprecation warn — the phase-254 pattern. Test:
  `plan_system_reads_lifecycle_from_system_toml`. cli suite green (395).
- **Wave 2 — `[param_persistence]` → typed — DONE (2026-06-17).** Added
  `SystemParamPersistence { backend, path }` + `param_persistence: Option<…>` on `SystemToml`.
  `schema_plan_json` prefers the typed block (empty `path` ⇒ no persistence), falling back to the
  `nros.toml` overlay with a deprecation warn. Test:
  `plan_system_reads_param_persistence_from_system_toml`. cli suite green (396).
- **Wave 3a — planner target-awareness — DONE (2026-06-18).** The shared prerequisite for W3 +
  W8: `SystemToml::resolve_target(cli)` (`--target` → `[system].default_target` → sole
  `[deploy.<t>]` → `None`) + `PlanOptions::target` + `nros plan --target`, threaded into
  `schema_build_json(.., cli_target)`. **First consumer: per-deploy RMW** — `resolved_rmw` is now
  called with the resolved target, so `[deploy.<t>].rmw` finally reaches the plan (the phase-255
  W2 stub resolved at `target = None`, so it never did). Tests: `resolve_target_precedence`,
  `schema_build_json_resolves_per_deploy_rmw_via_target`. cli suite green (401). Unblocks W3 (build
  tuning) + W8 (domain/locator precedence), which now just add more `resolve_target`-keyed fields.
- **Wave 3 — `[build]` tuning → `[deploy.<t>]` — DONE (2026-06-18).** Added `profile` / `optimize`
  / `features` to `DeployTarget` (the Eq-clean scalars); `schema_build_json` reads them from the
  W3a-selected deploy target into `plan.build`, preferring over the DEPRECATED `[build]` overlay
  (warns). `migrate` carries the legacy block's tuning over. **No fixture declared `[build]`
  tuning** (verified — the `[build]` hits are board descriptors), so the migration is value-neutral;
  the point is to give per-target build tuning a typed home before the overlay is deleted (W9).
  Test: `schema_build_json_reads_build_tuning_from_deploy`. cli suite green (402). **Follow-up:**
  the `[build.cargo]` / `[build.cc]` per-layer tables + compile `cfg` (toml::Value, not Eq) are not
  yet on `DeployTarget` — a small W3 tail when a fixture needs them.
- **Wave 4 — `[[scheduling.contexts]]` → `[tiers]`. DECISION: A (tiers absorb the EDF fields),
  locked 2026-06-18.** Grounded finding: tier and context are the **same concern, split** — a
  callback `group` binds to BOTH (`schema_callbacks` → context id; node `callback_groups` → tier),
  and the context's RT-policy fields (`class`/`period`/`budget`/`deadline`/`deadline_policy`/`core`)
  are **emitted into the runtime `SchedContextSpec`** by `render_sched_context`, so they are NOT
  vestigial and cannot be dropped. The tier model has none of them. **Decision A:** extend `TierDef`
  with the RTOS-agnostic policy fields (flat: `class`/`period_us`/`budget_us`/`deadline_us`/
  `deadline_policy`/`core`), keeping per-RTOS `priority`/`stack_bytes` in `TierRtosSpec` (already
  there). The planner derives `PlanSchedContext` from the resolved tier; the overlay
  `[[scheduling.contexts]]` becomes a warn-fallback. (Rejected B = two tables: both bind the same
  `group` → collision. C = same fields under a `[tiers.<n>.rt]` sub-table — equivalent capability,
  not chosen for the extra nesting.) **Greenfield:** 0 examples declare `[[scheduling.contexts]]`,
  so no fixtures to migrate — the work is the `TierDef` extension + a `resolve_tiers`→`PlanSchedContext`
  lowering. `TierDef` lives in `nros-orchestration-ir` (shared with the `nros::main!()` macro), so
  the field add touches that crate.
- **Wave 5 — `[[shared_state]]` → DROPPED. DECISION: remove the feature, scoped out (2026-06-18).**
  shared_state is a raw in-process shared-memory primitive — **not a ROS concept.** nano-ros is an
  RT *ROS* client (graph = nodes + pub/sub + services + actions + params + lifecycle); ROS 2's own
  answer for fast co-located comms is **intra-process zero-copy pub/sub** (loaned messages), which
  is in-paradigm. The exploration also showed inlining struct layouts in `system.toml` doesn't
  scale (types belong in code) — but moving the type to code (M1 cbindgen / M2 interface type) only
  underlined that the whole mechanism sits outside ROS. **Zero real users** — only the
  `shared_state_xlang` test fixture; no example/board/port adopts it. So instead of migrating it
  into the typed config, the feature is **removed**: RFC-0015 §8 deprecated; the schema
  (`SharedStateDecl`/`SharedStateField`), the planner path (`collect_shared_state` →
  `PlanSharedRegion` → `render_shared_state`), the bake codegen (`emit_shared_state_*`), the runtime
  `SharedRegion`/`LockedSharedRegion`, and the fixture all come out — tracked by **issue 0079**.
  Bonus: removes the `sync = "tier_aware"` coupling to W4's tiers. (The raw `{id,bytes}` overlay
  path also dies with W9's `nros.toml` deletion regardless.)
- **Wave 6 — `nros config show` — DONE (2026-06-17).** Added `nros config show --system <pkg>`
  (+ `--workspace`): prints the **resolved effective config** for a bringup system (rmw / domain /
  locator + the safety / param_services / lifecycle / param_persistence axes) with a **provenance
  column** (`system.toml [section]` vs built-in `default`), and flags any sibling `nros.toml`
  legacy overlay by NAME + the blocks it still carries (the Wave-0 `last_block_source` primitive,
  end-to-end). The legacy `config.toml` surface (88 embedded examples + book) is untouched when
  `--system` is absent. Rendered via a testable `render_resolved` (returns `String`). Tests:
  `render_resolved_shows_provenance_and_flags_legacy_overlay`, `render_resolved_errors_on_unknown_system`.
  cli suite green (398).
- **Wave 7 — `nros check` legacy-overlay audit — DONE (2026-06-17).** `nros check` now audits the
  `nros.toml` sitting next to a checked `system.toml` (the system.toml check path AND the
  `--bringup` / cwd-bringup auto-detect paths) for any still-declared legacy block
  (`build`/`lifecycle`/`param_persistence`/`param_services`/`safety`/`scheduling`/`shared_state`),
  emitting one warning per block — naming the file + the migration target (RFC-0004 §3.1, removed
  after the next release). Non-fatal (audit guard, not a hard error). Uses the Wave-0
  `last_block_source` primitive. Test: `legacy_overlay_audit_names_deprecated_blocks`. cli suite
  green (399).
- **Wave 8 — `domain_id`/`locator` per-deploy override — DONE (2026-06-18).** Added
  `domain_id`/`locator` to `DeployTarget` + `SystemToml::resolved_domain_id(target)` /
  `resolved_locator(target)` (the RFC-0004 §3.1 ladder, like `resolved_rmw`). The C bake
  (`render_system_config_h` → `#define NROS_SYSTEM_DOMAIN_ID`/`_LOCATOR`) and the vendor-hint
  `render_plan_json` both resolve through them for the selected `--target`, so the deploy override
  reaches the defines (was silently the `[system]` value). The Cargo-native projection
  (`[package.metadata.nros.deploy.<t>].domain_id`/`.locator`) now flows into the synthesized
  `DeployTarget` (`nros_config`), `migrate` carries the legacy fields. Tests:
  `resolved_domain_and_locator_honour_deploy_override`,
  `system_config_h_domain_locator_honour_deploy_override`. cli suite green (404). (The full
  precedence-vs-Cargo-projection surfacing in `config show` rides on W6's provenance — a small tail.)
- **Wave 9 — retire the legacy files (re-scoped).** Once every overlay block is migrated
  (W1-W5), the overlay readers go from warn-fallback → removed. Then, per the grounded re-scope:
  **(a) delete the `nros.toml` file support outright** — `package_nros_toml` / `load_toml_values`
  overlay path / the `nros.toml`-next-to-`system.toml` discovery — there is no surviving role
  (the §5 embedded-runtime job is deploy metadata). **(b) Scrub the `config.toml` reader**
  (`nros config show --config` / `nros config check --config`) — serves a file no example ships.
  **(c) Fold transport/network into the `deploy` class** — drop the phantom `[[transport]]` file
  home; the genuinely-needed multi-session topology lives under `system.toml` (with
  `[[domain]]`/`[[bridge]]`). RFC-0004 records the four-surface taxonomy (done in the re-scope).

## Acceptance

- Each migrated block declared **once** in `system.toml` (typed); both the planner and the bake
  resolve it from there. The per-package `nros.toml` overlay is a warn-fallback, then **the file
  support is removed entirely** (not narrowed).
- `nros config show --system <pkg>` prints the resolved config with a provenance column. ✓ (W6)
- `nros check` warns on any overlay-sourced value, naming the file. ✓ (W7)
- Generated output byte-identical for a system whose overlay values already equal the resolved
  `system.toml` values (the migration is value-preserving).
- **0 references to `nros.toml` and the legacy `config.toml` reader remain** — the live config
  surfaces are exactly the four in RFC-0004 §9 + Kconfig.

## Risks / decisions

- **Wave 4 (scheduling) is the design crux.** The tier model (RFC-0015) and the context model
  (period/budget/deadline) are not 1:1. If the runtime still consumes the timing fields, extend
  `TierDef`; if they were vestigial, drop them. Pin this against RFC-0015 §4.2 + the executor's
  actual scheduling inputs before coding.
- **Transport home — RESOLVED by the grounded sweep.** `[[transport]]` is NOT a separate file
  surface: 0 examples declare one, and embedded net config is expressed as `[..deploy.<t>]` fields
  (`ip`/`gateway`/`netmask`/`locator`). So transport/network is part of the **`deploy` class**
  (W3). The `[[transport]]` *schema* survives only for explicit multi-session / cross-RMW topology
  (planner overlay + `validate_transports`), and that genuinely-needed bit lives under
  `system.toml` next to `[[domain]]`/`[[bridge]]` — not a `nros.toml` file block. (RFC-0004 §6
  updated.)
- **Scope.** Large. The mechanical waves (0-2, 5) are low-risk and land first; Waves 3-4 carry
  the design weight; 6-8 are the audit surface; 9 is the cleanup. Each wave is independently
  landable and value-positive (one more concern leaves the overlay).
