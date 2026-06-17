# Phase 255 — RMW selection: one config source, both codegen paths

Status: **Design (2026-06-17)** · Implements [RFC-0031](../design/0031-rmw-selection-and-lowering.md)
precedence + [RFC-0004](../design/0004-configuration-and-transports.md) SSoT · Follows
[phase-254](phase-254-config-ssot-unify-codegen-paths.md) (which unified the *capability* axes;
this does RMW) · Tracked by [issue 0076 §A](../issues/0076-followups-config-ssot-and-safety-e2e-arc.md).

## Why — the RMW duality

RMW is declared in **two decoupled places** for the two codegen paths (worse than the
capability split phase-254 fixed):

| | Build / Rust path | C/C++ bake path |
| --- | --- | --- |
| Declared in | `[build].rmw` (per-package **`nros.toml`** overlay) | `[system].rmw` (**`system.toml`**) |
| Read by | `schema_build_json` → `plan.build.rmw` (`planner.rs`) | `render_system_config_h` (`codegen_system.rs`) |
| Lowers to | board crate `rmw-<x>` feature (`board_rmw_features` → `rmw_set(&plan.build)`) | `#define NROS_SYSTEM_RMW_<TOKEN>` |
| Default | `zenoh` | `zenoh` |
| Override | `[[transport]].rmw` (build overlay, bridge mode) | **`[deploy.<t>].rmw` — declared but UNUSED** |

The two are **fully decoupled** — no bridge. A user could set `[system].rmw = "cyclonedds"` and
`[build].rmw = "zenoh"` and get a C define for cyclone but a Rust build for zenoh. The
RFC-0031 precedence (`--rmw` > `[deploy]` > `[system]` > default) is **unimplemented**: no
`--rmw` flag, `[deploy.<t>].rmw` never read.

## Config format (the design)

**One declared home: `[system].rmw` + per-deploy `[deploy.<t>].rmw`. Both paths read it.**

```toml
[system]
rmw = "zenoh"              # the system's default backend (one binary links exactly one RMW)

[deploy.native]
rmw = "cyclonedds"         # optional per-deploy override of [system].rmw

[deploy.qemu-stm32]
# rmw omitted → inherits [system].rmw = "zenoh"
```

**Resolution (RFC-0031 §4.3, now implemented):**

```
rmw(target) = --rmw <x>                         (CLI flag, top)
            ?? [deploy.<selected-target>].rmw   (per-deploy override)
            ?? [system].rmw                      (system default)
            ?? "zenoh"                           (built-in default)
```

The **selected target** is the `[deploy.<t>]` block the build/bake is producing (the same
`--target` / `default_target` the rest of the deploy resolution already uses). Both paths
resolve identically through `resolve_rmw` (the existing SSoT mapping → cargo feature / CMake
value / C define token).

**Multi-RMW (bridge).** A single binary linking 2+ backends is the **`[[bridge]]`** case
(typed in `system.toml`): the build links the union of the RMWs named by the bridge's `connect`
endpoints. This replaces the build-overlay `[[transport]].rmw` multi-RMW path — bridges are
topology, so they belong in `system.toml`, read by both paths.

**Deprecated, then retired (RFC-0004 §3.1 — SSoT, not overlay).** `[build].rmw` /
`[[transport]].rmw` in `nros.toml` become a **fallback that warns**, then are removed — the
config model is SSoT-per-concern, and a per-package `nros.toml` overlay setting system RMW is
the action-at-a-distance hazard §3.1 forbids. `nros.toml` reverts to the §6 embedded-runtime
role only. The retirement is guarded by `nros check` flagging any overlay-sourced RMW + `nros
config show` surfacing the resolved value's provenance (issue 0076 §A).

## Waves

- **Wave 1 — `SystemToml::resolved_rmw(target, cli)` — DONE (2026-06-17).** Added `rmw:
  Option<String>` to `DeployTarget` (system.toml `[deploy.<t>]` had none) + the `resolved_rmw`
  helper applying the precedence (CLI → `[deploy.<t>].rmw` → `[system].rmw` → `zenoh`). The
  Cargo-native projection (`[package.metadata.nros.deploy.<t>].rmw`) flows into the synthesized
  `DeployTarget` (`nros_config.rs`). Test: `resolved_rmw_precedence_ladder` (each rung). No
  behaviour change yet (helper unused until Waves 2-3). cli suite green (387).
- **Wave 2 — planner reads it — DONE (2026-06-17).** `schema_build_json(overlays, system_toml)`
  sets `plan.build.rmw` from `SystemToml::resolved_rmw(None, None)` (= `[system].rmw` today; the
  planner is target-agnostic, so `[deploy.<t>].rmw`/`--rmw` plumb in with target-awareness —
  issue 0076 §A), **preferring** it over the `[build].rmw` overlay (deprecated, warns). Test:
  `schema_build_json_system_toml_rmw_wins_over_build_overlay`. **This already fixes a live
  duality bug:** `multi_pkg_workspace_esp_idf/system.toml` declares `rmw = "xrce"` but had no
  `[build].rmw`, so the plan defaulted to `zenoh` (≠ the C bake's xrce); now the plan resolves to
  `xrce`. cli suite green (388). (The esp_idf *bringup* test needs a prebuilt fixture — a
  pre-existing env precondition, unrelated.)
- **Wave 3 — bake reads it — DONE (2026-06-17).** `render_system_config_h(sys, target)` resolves
  RMW through `SystemToml::resolved_rmw(target, None)` — the SAME helper the planner uses — so the
  C `#define NROS_SYSTEM_RMW` / `NROS_SYSTEM_RMW_<TOKEN>` honour `[deploy.<target>].rmw`, not just
  `[system].rmw`. The selected `--target` (already threaded into `emit_bake_tree`) is the deploy
  key. Test: `system_config_h_rmw_honours_deploy_override` (deploy override wins for the target;
  `[system].rmw` default with no target). cli suite green (389).
- **Wave 4 — `--rmw` CLI flag — DONE (2026-06-17).** Added `--rmw <x>` to both `nros plan`
  (`PlanOptions::rmw` → `schema_build_json(.., cli_rmw)`) and `nros codegen-system`
  (`emit_bake_tree(.., cli_rmw)` → `render_system_config_h(sys, target, cli_rmw)`). It is the TOP
  of the ladder — `resolved_rmw(target, Some(cli))` returns `cli` regardless of `system.toml`;
  with no `system.toml` the plan still honours `--rmw`. Tests:
  `schema_build_json_cli_rmw_tops_the_ladder`, `system_config_h_rmw_honours_deploy_override`
  (extended with the `--rmw` rung). cli suite green (390).
- **Wave 5 — multi-RMW via `[[bridge]]` — DONE (2026-06-17).** `SystemToml::bridged_rmws()`
  returns the union of the system default plus every cross-RMW `[[bridge]]` endpoint's RMW (the
  `<rmw>:<domain>` prefix, or a bare `[[domain]]` name → its `rmw`). `schema_build_json` records it
  as the plan's `PlanBuildOptions::bridged_rmws` (skip-when-empty → single-RMW builds
  byte-identical), and `rmw_set` (board-feature lowering) folds it into the linked backend set
  alongside `build.rmw` / `[[transport]].rmw`. Tests: `bridged_rmws_unions_bridge_endpoints`,
  `schema_build_json_emits_bridged_rmws_from_system_toml`, `rmw_set_unions_bridged_rmws`. cli suite
  green (393). The `[[transport]].rmw` overlay multi-RMW path stays *readable* during the
  transition (Wave 6 retires it); `[[bridge]]` is now the authoritative SSoT for multi-RMW.
- **Wave 6 — migrate fixtures + docs.** Move `[build].rmw` declarations to `system.toml`
  `[system].rmw`/`[deploy]`; RFC-0004 §4 + RFC-0031 §4.3 record the implemented precedence +
  single source; retire the `[build].rmw` overlay after the release.

## Acceptance

- RMW declared **once** (`[system].rmw` + `[deploy.<t>].rmw`); both the board-feature lowering
  and the C `#define` resolve from it through `resolve_system_rmw`.
- `[deploy.<t>].rmw` overrides `[system].rmw` for that target (both paths); `--rmw` overrides
  both. Precedence unit-tested.
- A bridge's multi-RMW link set comes from `[[bridge]]` (`system.toml`), not a `nros.toml`
  overlay.
- Generated output byte-identical for a system whose `[build].rmw` already equals its
  resolved `[system].rmw`.

## Risks / decisions

- **Per-deploy RMW means the plan is target-scoped.** `plan.build.rmw` already keys off the
  selected target/board; resolving RMW the same way is consistent — confirm the target is in
  scope at `schema_build_json` (it gets `board`/`target` from the same overlay today).
- **Bridge reconciliation.** `[[transport]].rmw` (build overlay) and `[[bridge]]` (system.toml)
  both express multi-RMW; Wave 5 makes `[[bridge]]` authoritative. Keep `[[transport]]`
  readable during the transition.
- **Scope.** This phase does RMW only; the remaining overlay blocks (`[lifecycle]`,
  `[param_persistence]`, scheduling, the rest of `[build]`) are issue-0076 §A, a later phase.
