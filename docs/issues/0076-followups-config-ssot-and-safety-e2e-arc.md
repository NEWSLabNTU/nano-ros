---
id: 76
title: Follow-up tracker — config SSoT consolidation + safety-e2e capability arc
status: open
type: tracking
area: build
related: [phase-250, phase-252, phase-254, phase-255, phase-256, issue-0072, issue-0073, rfc-0004, rfc-0031]
---

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

**All five items below are now designed + waved in
[phase-256](../roadmap/phase-256-config-ssot-retire-overlay-blocks.md)** (the §3.1 endgame, same
pattern as phase-254/255). The block map (reader fn + typed-field status per concern) lives there.

- [ ] **Retire ALL legacy `nros.toml` build-overlay blocks → `system.toml` (typed).** Not just
  `[safety]`/`[param_services]` (done, phase-254) and `rmw` (done, phase-255): also `[build]` rest
  (target/board/profile/optimize/cargo/cc/features/`[[transport]]`), `[lifecycle]`,
  `[param_persistence]`, `[[scheduling.contexts]]`, `[[shared_state]]`. Each moves to a typed
  `system.toml` field/table; the overlay read becomes a **warning fallback**, then is removed.
  **phase-256 Waves 1-4** (`lifecycle` DONE; `build` rest → `[deploy.<t>]`, DONE; `scheduling` →
  `[tiers]` SSoT = W4, decision A). **`[param_persistence]` DISABLED** — in scope but incomplete,
  no embedded `ParamStore` backends (issue 0080). **`[[shared_state]]` REMOVED** — out of ROS scope
  (issue 0079).
- [ ] **`nros config show`** — print the **resolved effective config** for a system + **per-value
  provenance** (which file each value came from). The audit backstop for SSoT (RFC-0004 §3.1).
  Today's `nros config` reads the retired pre-212 `config.toml`; this is the new-model command.
  **phase-256 Wave 6** (needs the Wave 0 provenance primitive — `load_toml_values` source-tagging).
- [ ] **`nros check` flags legacy-overlay-sourced values** — any value still coming from a
  per-package `nros.toml` overlay surfaces a warning + removal date (the action-at-a-distance
  guard). Extends `check`'s current plan/schema validation. **phase-256 Wave 7.**
- [ ] **Deploy-metadata precedence (leakage).** `[package.metadata.nros.deploy.<t>]` (`rmw`,
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
- [ ] **Migrate the other overlay blocks to typed `system.toml`** — `[build]` rest, `[lifecycle]`,
  `[param_persistence]`, `[[scheduling.contexts]]`, `[[shared_state]]`. The full RFC-0004
  endgame; **phase-256** (designed), same pattern as phase-254/255.
- [ ] **Retire the deprecated per-package `nros.toml` capability/RMW-overlay fallback** — kept
  one release by phase-254 Wave 2 + phase-255 (warns). Remove once nothing uses it (RFC-0004 §5:
  `nros.toml` is the embedded direct-mode runtime file only). **phase-256 Wave 9.**

## B. safety-e2e tails

- [ ] **threadx boards safety wiring** — `nros-board-threadx-{linux,qemu-riscv64}` expose no
  `rmw-zenoh` board feature (non-standard backend wiring), so `[safety]` is not advertised; the
  descriptor gate skips + warns. Needs threadx's backend wiring understood before forwarding
  (phase-252 Wave 4 skip).
- [ ] **cyclonedds / xrce have no safety-e2e CRC path** — the axis no-ops there (documented in
  `cyclonedds-known-limitations.md`). A DDS-side CRC + C surface is unscoped (issue 0073).
- [ ] **C++ safety transport e2e** — the C transport e2e proves the validation; the C++ ABI
  calls the same `RmwSubscriber::try_recv_validated`, so no separate C++ e2e was added. Add one
  if the C++ path needs independent CI coverage (issue 0073).
- [ ] **Generic declared-feature config sugar** — a `features = [...]` list over the
  `resolve_capability` registry (RFC-0031 §Generalization future note).

## C. Older residuals (pre-arc, still open)

- [ ] **macOS cyclonedds `--allow-multiple-definition` removal** — phase-249 D3 tail; the macOS
  cyclonedds branch (`CMakeLists.txt`, `-force_load` + flag) needs a macOS run to validate
  removal. Issue 0072 (resolved) notes the Linux/BSD removal landed.
- [ ] **Issue 0050 (weak-symbol audit)** — archive on the next cut (its W-items landed).

## Notes

Each box is independently landable. **A**: capabilities (phase-254) + RMW (phase-255) are DONE;
the remaining overlay blocks + the `config show` / `check` / deploy-precedence audit surface are
designed in **phase-256** (the §3.1 endgame). Pick from phase-256's mechanical waves (lifecycle /
param_persistence / shared_state) for the lowest-risk value; Waves 3-4 (build-rest, scheduling)
carry the design weight.
