---
id: 76
title: Follow-up tracker — config SSoT consolidation + safety-e2e capability arc
status: resolved
type: tracking
area: build
related: [phase-250, phase-252, phase-254, phase-255, phase-256, phase-259, phase-260, phase-261, issue-0072, issue-0073, rfc-0004, rfc-0031]
resolved_in: "§A phase-254/255/256 · §B phase-259 (W4→phase-261) · §C phase-260 (macOS drop) + 0050 archived"
---

## Resolution (2026-06-18) — all arcs landed; one deferred future-note

- **§A config-SSoT** — DONE (phase-254 capabilities, phase-255 RMW, phase-256
  W0-W9: lifecycle/build/scheduling → typed `system.toml`, `nros config show`,
  `nros check` overlay audit, legacy-file retirement). param_persistence → 0080,
  shared_state → 0079.
- **§B safety-e2e tails** — DONE (phase-259: threadx false-warning removed, loud
  no-CRC gate, C++ e2e). W4 (declared-feature multi-language sugar) spun to
  **phase-261**, deferred (YAGNI — one axis).
- **§C residuals** — macOS dropped (**phase-260**), 0050 archived.

No actionable open work remains; the lone tail (phase-261 W4) is independently
tracked + intentionally deferred. Archived.

## Why

The safety-e2e capability arc (phase-250 → crc fix → phase-252 → issue-0073 → phase-254)
landed the capability + the config-SSoT unification for capabilities. This tracks the
remaining tails, grouped, so they don't scatter.

## A. Config SSoT consolidation (RFC-0004 §3.1 endgame)

**Design decision (2026-06-17): nano-ros is SSoT-per-concern, NOT an overlay system**
(RFC-0004 §3.1). The legacy Phase-172 per-package `nros.toml` build/capability overlay is
action-at-a-distance (a value set in some package's file silently changes the build) and
contradicts RFC-0004 (`nros.toml` is the embedded-runtime file only). It is being **retired**,
not merely supplemented. phase-254 did the capability axes; the rest of §A finishes it.

**§A is COMPLETE (2026-06-18) via
[phase-256](../roadmap/phase-256-config-ssot-retire-overlay-blocks.md)** (the §3.1 endgame, same
pattern as phase-254/255) — all waves W0-W9 DONE. The block map (reader fn + typed-field status per
concern) lives there. Two sub-concerns spun out: `[param_persistence]` disabled pending embedded
backends (issue 0080), `[[shared_state]]` removed as out-of-scope (issue 0079). Remaining 0076
work = §B (safety-e2e tails) + §C (residuals).

- [x] **Retire ALL legacy `nros.toml` build-overlay blocks → `system.toml` (typed).** DONE — phase-256 W1-9 (a sweep found 0 `nros.toml`/`config.toml` files in `examples/**`; both legacy files retired). Not just
  `[safety]`/`[param_services]` (done, phase-254) and `rmw` (done, phase-255): also `[build]` rest
  (target/board/profile/optimize/cargo/cc/features/`[[transport]]`), `[lifecycle]`,
  `[param_persistence]`, `[[scheduling.contexts]]`, `[[shared_state]]`. Each moves to a typed
  `system.toml` field/table; the overlay read becomes a **warning fallback**, then is removed.
  **phase-256 Waves 1-4** (`lifecycle` DONE; `build` rest → `[deploy.<t>]`, DONE; `scheduling` →
  `[tiers]` SSoT = W4, decision A). **`[param_persistence]` DISABLED** — in scope but incomplete,
  no embedded `ParamStore` backends (issue 0080). **`[[shared_state]]` REMOVED** — out of ROS scope
  (issue 0079).
- [x] **`nros config show`** — DONE — phase-256 W6 (`nros config show --system <pkg>`, `cmd/config.rs`). print the **resolved effective config** for a system + **per-value
  provenance** (which file each value came from). The audit backstop for SSoT (RFC-0004 §3.1).
  Today's `nros config` reads the retired pre-212 `config.toml`; this is the new-model command.
  **phase-256 Wave 6** (needs the Wave 0 provenance primitive — `load_toml_values` source-tagging).
- [x] **`nros check` flags legacy-overlay-sourced values** — DONE — phase-256 W7 (`legacy_warnings` in `cmd/check.rs`). any value still coming from a
  per-package `nros.toml` overlay surfaces a warning + removal date (the action-at-a-distance
  guard). Extends `check`'s current plan/schema validation. **phase-256 Wave 7.**
- [x] **Deploy-metadata precedence (leakage).** DONE — phase-256 W8 (per-deploy `domain_id`/`locator` override, explicit ladder). `[package.metadata.nros.deploy.<t>]` (`rmw`,
  `domain_id`, `locator`) + `[workspace.metadata.nros]` (`rmw_override`, `domain_id_override`)
  are the **single-node Cargo-native projection**. When a `system.toml` exists for the same
  scope it is authoritative (the RFC-0004 §3.1 ladder: flag > `system.toml` > native projection
  > default) — make this explicit + non-silent, not an overlay merge. **phase-256 Wave 8.**

The original capability/RMW items (now under the §3.1 umbrella):

- [x] **RMW duality → one SSoT — `[system].rmw` / `[deploy.<t>].rmw`.** **DONE — phase-255**
  (all 6 waves). `SystemToml::resolved_rmw(target, cli)` is read by BOTH the planner (board
  `rmw-<x>` feature) and the bake (`#define NROS_SYSTEM_RMW`).
- [x] **Wire `[deploy.<t>].rmw`** — **DONE — phase-255** (`DeployTarget.rmw` read via
  `resolved_rmw`; per-deploy override of `[system].rmw`).
- [x] **`--rmw` CLI flag** — **DONE — phase-255 Wave 4** (`nros plan` + `nros codegen-system`;
  top of the precedence ladder).
- [x] **Migrate the other overlay blocks to typed `system.toml`** — DONE — phase-256 (`[build]` rest
  → `[deploy.<t>]`, `[lifecycle]`, `[[scheduling.contexts]]` → `[tiers]`; `[param_persistence]`
  disabled → 0080; `[[shared_state]]` removed → 0079).
- [x] **Retire the deprecated per-package `nros.toml` capability/RMW-overlay fallback** — DONE — phase-256 W9 (orchestration scope; 0 `nros.toml` files exist). kept
  one release by phase-254 Wave 2 + phase-255 (warns). Remove once nothing uses it (RFC-0004 §5:
  `nros.toml` is the embedded direct-mode runtime file only). **phase-256 Wave 9.**

## B. safety-e2e tails

**W1-W3 done in [phase-259](../roadmap/archived/phase-259-safety-e2e-tails.md)**
(threadx false-warning removed, loud no-CRC gate, C++ e2e). **W4** (declared-feature
multi-language sugar) spun out to
[phase-261](../roadmap/phase-261-capability-registry-multilang-lowering.md),
deferred.

- [x] **threadx boards safety wiring** — DONE (phase-259 W1). threadx is app-level RMW; the
  backend dep (`render_backend_dependencies` → `nros-rmw-zenoh[safety-e2e]`) carries the CRC
  regardless of board advertisement, so threadx+zenoh+`[safety]` forwards. Removed the false
  board-level "NOT backend CRC" warning; W2 is the accurate (resolved-RMW) signal.
- [x] **cyclonedds / xrce have no safety-e2e CRC path** — DONE (phase-259 W2). The axis no-ops
  there (documented in `cyclonedds-known-limitations.md`) AND now warns loudly at plan/check time
  when `[safety]` is declared on a non-CRC resolved RMW (`collect_plan_warnings`).
- [x] **C++ safety transport e2e** — DONE (phase-259 W3). `examples/native/cpp/safety-listener/`
  + `test_cpp_safety_listener_validates_crc` (green: `cpp safety: 3 crc-ok, 0 crc-fail`).
- [ ] **Generic declared-feature config — MULTI-LANGUAGE registry generalization** — a
  `features = [...]` list over the `Capability` registry. NOT a Rust-only sugar: the registry is
  Rust-specific today (cargo-feature slots only; the C/C++ `#define` lowering is hardcoded per-axis
  in `render_system_config_h`, with `c_define`/`cmake_token` reserved). The real W4 adds the
  reserved C/C++ slots + makes the bake iterate them, so one `Capability{}` row lowers to Rust
  features AND the C/C++ `#define`/CMake token. **DEFERRED** (YAGNI — only one concrete axis
  today; revisit when a 2nd lands). Filed as its own phase:
  [phase-261](../roadmap/phase-261-capability-registry-multilang-lowering.md).

## C. Older residuals (pre-arc, still open)

- [x] **macOS cyclonedds `--allow-multiple-definition` removal** — RESOLVED by **dropping macOS
  support** ([phase-260](../roadmap/phase-260-drop-macos-support.md)). The macOS `elseif(APPLE)`
  `-force_load` cyclone branches (+ the `NOT APPLE` stdc++ guards, the posix/custom-platform APPLE
  link branches, and the release darwin targets) are removed — no macOS = nothing to validate, the
  unvalidatable branch is gone rather than pending a runner. (W3 rust-cfg / W4 doc sweep tracked in
  phase-260.)
- [x] **Issue 0050 (weak-symbol audit)** — DONE — already resolved + archived
  (`archived/0050-*`; phase-247 gates + phase-249 P4a + 2026-06-15 re-audit). Gates live:
  `check-weak-symbols` (in check-fast) + `check-weak-symbols-image`.

## Notes

Each box is independently landable. **A**: capabilities (phase-254) + RMW (phase-255) are DONE;
the remaining overlay blocks + the `config show` / `check` / deploy-precedence audit surface are
designed in **phase-256** (the §3.1 endgame). Pick from phase-256's mechanical waves (lifecycle /
param_persistence / shared_state) for the lowest-risk value; Waves 3-4 (build-rest, scheduling)
carry the design weight.
