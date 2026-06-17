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

- **Wave 1 — `resolve_system_rmw(system_toml, target, cli_flag)`** — one helper that applies the
  precedence (CLI → `[deploy.<t>].rmw` → `[system].rmw` → `zenoh`) over the typed `SystemToml`.
  Unit tests for each precedence rung. No behaviour change yet.
- **Wave 2 — planner reads it.** `schema_build_json` / `plan.build.rmw` derive from
  `resolve_system_rmw` (the bringup `system.toml` + selected target), preferring it over the
  `[build].rmw` overlay (deprecated fallback, warns). So the board `rmw-<x>` feature lowering is
  driven by `[system].rmw`/`[deploy].rmw`. Byte-identical for plans that already match.
- **Wave 3 — bake reads it.** `render_system_config_h` resolves through the same
  `resolve_system_rmw` (honouring `[deploy.<t>].rmw`, not just `[system].rmw`), so the C define
  matches the build for a given target.
- **Wave 4 — `--rmw` CLI flag.** Add to `nros plan` / `nros codegen-system` `Args`; top of the
  precedence. Threaded into `resolve_system_rmw`.
- **Wave 5 — multi-RMW via `[[bridge]]`.** The build links the union of the bridge endpoints'
  RMWs from `system.toml`; deprecate the `[[transport]].rmw` overlay multi-RMW path.
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
