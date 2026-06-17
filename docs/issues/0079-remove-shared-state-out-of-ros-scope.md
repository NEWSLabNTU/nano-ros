---
id: 79
title: Remove shared_state — raw in-process shared memory is out of nano-ros (RT ROS) scope
status: open
type: tech-debt
area: orchestration
related: [phase-256, rfc-0015, phase-228]
---

## Decision (2026-06-18)

`shared_state` (RFC-0015 §8, Phase 228.D) is **removed**. It is a raw in-process
shared-memory primitive — **not a ROS concept**. nano-ros is an RT *ROS* client:
the computation graph is nodes + pub/sub + services + actions + params + lifecycle.
ROS 2's own answer for fast co-located comms is **intra-process zero-copy pub/sub**
(loaned messages), which is in-paradigm; a bespoke shared-struct-with-mutex is a
parallel non-ROS IPC mechanism that crept in.

Confirmed during the phase-256 config tidy: **zero real users** — only the
`shared_state_xlang` test fixture; no example, board, or downstream port adopts it.
The exploration of "define the struct in code, not config" (M1 cbindgen / M2
interface type) only underlined that the whole mechanism sits outside ROS.

## Scope — what comes out

- **Schema:** `SharedStateDecl`, `SharedStateField` (`nros-orchestration-ir`);
  `SystemToml::shared_state` field; the `nros.toml` `[[shared_state]]` overlay
  (`collect_shared_state`).
- **Planner path:** `PlanSharedRegion`, `NrosPlan::shared_state`,
  `render_shared_state` (`generate.rs`).
- **Bake path:** `shared_state_docs`, `emit_shared_state_rust`,
  `emit_shared_state_c_header`, `PlanSharedStateDoc` (`codegen_system.rs`).
- **Runtime:** `SharedRegion` / `LockedSharedRegion` (`nros-orchestration`).
- **CLI surface:** the `shared_state` entries in `check.rs` (legacy-overlay audit
  block list), `config.rs` (`render_resolved` audit), `migrate.rs`, `new_system.rs`.
- **Fixture + tests:** `packages/testing/nros-tests/fixtures/shared_state_xlang/`
  and the `shared_state` unit tests in `planner.rs` / `codegen_system.rs`.
- **Docs:** RFC-0015 §8 → deprecated/removed; phase-228 note; RFC-0004 mentions.

## Notes

- Removes the `sync = "tier_aware"` coupling between shared_state and the
  scheduling tiers (simplifies phase-256 W4, the tier model).
- The raw `{id,bytes}` overlay path also dies with phase-256 W9 (`nros.toml`
  deletion) regardless; this issue is the full feature removal, not just the overlay.
- If a future need for in-paradigm fast co-located comms arises, the answer is an
  **intra-process zero-copy pub/sub** RFC, not reviving this.
