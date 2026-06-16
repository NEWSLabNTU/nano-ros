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

## A. Config SSoT consolidation (RFC-0004 endgame)

phase-254 unified the **capability** axes (`[safety]`, `[param_services]`) onto `system.toml`,
read by both codegen paths. The same model should cover the rest:

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
