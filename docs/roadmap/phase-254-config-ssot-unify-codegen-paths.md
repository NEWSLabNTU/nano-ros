# Phase 254 — one config SSoT: capabilities in system.toml, unify the two codegen paths

Status: **Planned (2026-06-16)** · Implements [RFC-0004](../design/0004-configuration-and-transports.md)
(system.toml is the SSoT; both languages read the same file) + [RFC-0031 §"Generalization"](../design/0031-rmw-selection-and-lowering.md)
(declared capability axes) · Follows [phase-250](phase-250-safety-params-feature-dimension.md),
[phase-252](phase-252-capability-axis-board-lowering.md), closes the config-model tail of
[issue 0072](../issues/0072-safety-e2e-backend-feature-not-lowered.md) / [issue 0073](../issues/0073-safety-e2e-c-cpp-cmake-path-missing.md).

## Why

Two orchestration codegen paths read **different config sources**, so a declared capability
axis (`[safety]`, `[param_services]`) reaches only one of them:

| Path | Reads | Sees `[safety]`? |
| --- | --- | --- |
| Rust — `planner` → `NrosPlan` → `generate.rs` | per-package **`nros.toml` overlays** (`collect_safety`, `planner.rs:94-98`) | yes |
| C/C++ bake — `cmd/codegen_system.rs` → `render_system_config_h` | typed **`SystemToml`** only (`bringup.system`) | **no** |

Consequences:
- **Capabilities are Rust-only.** A C/C++ bake never sees `[safety]` → no `#define
  NROS_SYSTEM_SAFETY_E2E`; the C/C++ safety capability (issue 0073) has no config-driven knob,
  only the `NANO_ROS_SAFETY_E2E` CMake flag.
- **RMW is declared twice** — `[system].rmw` (system.toml, for the bake) AND `[build].rmw`
  (nros.toml overlay, for Rust). Consistent only by user discipline.
- **It contradicts RFC-0004.** RFC-0004 §2-3 makes `system.toml` the SSoT and says C/C++ +
  Rust "both read the same `system.toml`"; §5 scopes `nros.toml` to the **embedded
  direct-mode runtime file only** (board parses at boot — transports/RT). The planner's
  per-package `nros.toml` **capability-overlay** read is the legacy Phase-172 path that
  RFC-0004 supersedes but the code never finished migrating (root `nros.toml` was retired in
  issue 0051, the per-package capability overlays survived).

## Design (Option A — RFC-0004-pure: one typed SSoT, both paths read it)

1. **Capability axes are typed `system.toml` fields.** `[safety]` / `[param_services]` (and
   the `[build]`-vs-`[system]` RMW duplication) move into `SystemToml` — typed, so
   `deny_unknown_fields` accepts them and the bake reads them. Declared **once**.
2. **The planner sources capabilities from `system.toml`**, not the per-package `nros.toml`
   overlay. `plan.safety` / `plan.param_services` derive from the typed `SystemToml`.
3. **The bake reads the same `system.toml`** → `render_system_config_h` emits
   `#define NROS_SYSTEM_SAFETY_E2E` (+ the existing `NROS_SYSTEM_RMW_*`) from the one source.
4. **`nros.toml` reverts to its RFC-0004 role** — the embedded direct-mode runtime file only;
   it stops being a build-capability overlay.

Net: `[safety]` declared once in `system.toml` → both the Rust feature lowering (phase-250/252)
**and** the C/C++ `#define` fall out. Kills the RMW duality; makes capabilities visible to C/C++.

**Rejected — Option B** (make the bake ALSO read the lenient `nros.toml` overlays): smaller,
but keeps two schemas + two files, entrenches the wart, and contradicts RFC-0004's
single-`system.toml` SSoT + nros.toml-is-runtime-only stance.

## Waves

- **Wave 1 — typed capability schema in `SystemToml` — DONE (2026-06-16).**
  `SystemToml.safety: Option<SystemSafety { enabled, crc }>` + `param_services:
  Option<SystemParamServices { enabled }>` (both `deny_unknown_fields`, defaults true,
  skip-when-absent → byte-identical). Test: `parses_system_toml_capability_axes` (parse,
  defaults, `enabled = false` opt-out, round-trip, absent→None). No behaviour change yet
  (parsed, not yet consumed). cli suite green (384).
- **Wave 2 — planner reads capabilities from `system.toml` — DONE (2026-06-16).**
  `Package.system_toml` + `Workspace::package_system_toml` (discovery); `schema_plan_json`
  parses the bringup's typed `SystemToml` and derives `plan.safety` / `plan.param_services`
  from `[safety]` / `[param_services]`, **preferring** it over the per-package `nros.toml`
  overlay block — which is now a **deprecated fallback** (`eprintln!` warn) kept one release
  for migration. Test: `plan_system_reads_safety_from_system_toml` (system.toml `[safety]` →
  `plan.safety.crc`). Existing fixtures stay green via the fallback (385).
- **Wave 3 — bake emits the capability defines — DONE (2026-06-16).**
  `render_system_config_h` emits `#define NROS_SYSTEM_SAFETY_E2E` +
  `#define NROS_SYSTEM_PARAM_SERVICES` from the typed `system.toml` `[safety]` /
  `[param_services]` (the analog of `NROS_SYSTEM_RMW_<TOKEN>`), for C/C++ conditional
  compile. Test: `system_config_h_emits_capability_defines` (present, absent→none,
  `enabled=false`→none). **Both codegen paths now read the same `system.toml`** for
  capabilities. Closes the issue-0073 C-define follow-up. (386)
- **Wave 4 — migrate examples/fixtures + retire the overlay path.** Move declared `[safety]`
  etc. into the bringup `system.toml`; drop the per-package `nros.toml` capability blocks +
  the deprecated planner fallback. `nros.toml` is now runtime-only (RFC-0004 §5).
- **Wave 5 — docs.** RFC-0004 §4 schema gains the capability axes + the "both paths read
  system.toml" statement; RFC-0031 §Generalization records system.toml as the declared home;
  issue 0072/0073 tails closed.

## Acceptance

- `[safety]` / `[param_services]` declared **once** in `system.toml`; both the Rust feature
  lowering and the C/C++ `#define NROS_SYSTEM_SAFETY_E2E` derive from it.
- `codegen_system.rs` (bake) and `planner.rs` (Rust) read the **same** `system.toml` for
  capabilities — no per-path config divergence.
- `nros.toml` carries no build-capability overlays (RFC-0004 §5 runtime-only); the deprecated
  fallback is removed.
- RMW is declared once (`[system].rmw`); the `[build].rmw` overlay duplication is reconciled.
- Generated output stays byte-identical for plans that don't declare a capability.

## Risks

- **Migration churn.** Existing fixtures declare `[safety]` in per-package `nros.toml`; the
  Wave-2 deprecated fallback (warn, don't break) de-risks the transition before Wave-4 retires it.
- **RMW reconciliation.** `[build].rmw` is load-bearing for the Rust build path; fold to
  `[system].rmw` carefully (the bake already uses `[system].rmw`) — keep both readable during
  the transition, prefer `[system].rmw`.
- **Scope creep into the full overlay set.** `[lifecycle]`, `[param_persistence]`,
  `[[scheduling]]`, `[[shared_state]]` are also overlays — this phase scopes the **capability**
  axes (safety, param_services) + RMW reconciliation; the rest follow the same pattern in a
  later phase, not here.
