---
id: 76
title: Follow-up tracker — config SSoT consolidation + safety-e2e capability arc
status: open
type: tracking
area: build
related: [phase-250, phase-252, phase-254, phase-255, issue-0072, issue-0073, rfc-0004, rfc-0031]
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

- [ ] **Retire ALL legacy `nros.toml` build-overlay blocks → `system.toml` (typed).** Not just
  `[safety]`/`[param_services]` (done, phase-254): also `[build]` (target/board/profile/`[[transport]]`),
  `[lifecycle]`, `[param_persistence]`, `[[scheduling]]`, `[[shared_state]]`. Each moves to a
  typed `system.toml` field/table; the overlay read becomes a **warning fallback**, then is
  removed. After removal the `nros.toml` same-name collision (build-overlay vs §6 embedded-runtime)
  is gone.
- [ ] **`nros config show`** — print the **resolved effective config** for a system + **per-value
  provenance** (which file each value came from). The audit backstop for SSoT (RFC-0004 §3.1).
  Today's `nros config` reads the retired pre-212 `config.toml`; this is the new-model command.
- [ ] **`nros check` flags legacy-overlay-sourced values** — any value still coming from a
  per-package `nros.toml` overlay surfaces a warning + removal date (the action-at-a-distance
  guard). Extends `check`'s current plan/schema validation.
- [ ] **Deploy-metadata precedence (leakage).** `[package.metadata.nros.deploy.<t>]` (`rmw`,
  `domain_id`, `locator`) + `[workspace.metadata.nros]` (`rmw_override`, `domain_id_override`)
  are the **single-node Cargo-native projection**. When a `system.toml` exists for the same
  scope it is authoritative (the RFC-0004 §3.1 ladder: flag > `system.toml` > native projection
  > default) — make this explicit + non-silent, not an overlay merge.

The original capability/RMW items (now under the §3.1 umbrella):

- [ ] **RMW duality → one SSoT — `[system].rmw` / `[deploy.<t>].rmw`.** Today `[build].rmw`
  (per-package `nros.toml` overlay → board crate `rmw-<x>` feature) and `[system].rmw`
  (`system.toml` → `#define NROS_SYSTEM_RMW`) are **fully decoupled** for the two paths. The
  fix mirrors phase-254. **Design + impl: phase-255** (this issue's sibling); the config format
  is designed there + in RFC-0004/RFC-0031.
- [ ] **Wire `[deploy.<t>].rmw`** — declared in `DeployTarget` but **never read**. Per-deploy
  RMW override (RFC-0031 precedence). Part of phase-255.
- [ ] **`--rmw` CLI flag** — RFC-0031 precedence top, **unimplemented** (`nros plan` /
  `nros codegen-system` `Args` have no `--rmw`). Part of phase-255.
- [ ] **Migrate the other overlay blocks to typed `system.toml`** — `[build]`
  (target/board/profile/`[[transport]]`), `[lifecycle]`, `[param_persistence]`,
  `[[scheduling]]`, `[[shared_state]]`. The full RFC-0004 endgame; a later phase, same pattern
  as phase-254/255.
- [ ] **Retire the deprecated per-package `nros.toml` capability-overlay fallback** — kept one
  release by phase-254 Wave 2 (warns). Remove once nothing uses it (RFC-0004 §5: `nros.toml`
  is the embedded direct-mode runtime file only).

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

Each box is independently landable. **A (RMW duality)** is the active design — see phase-255.
The rest are deferred-but-tracked; pick from the top of A for the most value.
