---
id: 243
title: "Platform board-trait family duplicated during transition: nros-platform::board::{Board,BoardInit,BoardEntry} vs legacy nros-board-common::board_init::{…} both live"
status: open
type: tech-debt
severity: low
area: platform
related: [rfc-0034]
---

## Finding (RMW/platform API audit, 2026-07-21)

Two parallel board-lifecycle trait families are live simultaneously:

- **New** (Phase 212.N.1): `nros-platform::board::{Board, BoardInit,
  BoardEntry, BoardPrint, BoardExit, TransportBringup, NetworkWait, …}`
  (`packages/core/nros-platform/src/board/mod.rs:38-96`), where config
  moved off `BoardInit` into `RuntimeCtx` (`board/init.rs:11-21`).
- **Legacy**: `nros-board-common::board_init::{Board, BoardInit, …}` — kept
  "live during transition" (`board/mod.rs:40-48`), plus a
  `NodeRuntime` → `NodeDispatchRuntime` deprecated alias
  (`mod.rs:83-86`).

Duplicated trait surfaces during a migration are fine SHORT-term, but this
one has no recorded end-state or tracking, so it risks becoming permanent
two-of-everything (the exact antipattern the API audit flags): a new board
author has to know which `BoardInit` to implement, and downstream code
picks one arbitrarily.

## Direction
Record the convergence plan: which board crates still implement the legacy
family, what blocks their move to `nros-platform::board`, and a target for
deleting `nros-board-common::board_init` + the `NodeRuntime` alias. Then
either finish the migration or, if the legacy family is intentionally
permanent for some layer, document why and stop calling it "transition".

## Recorded state (2026-07-28) — the migration barely started; needs a decision

Audit of all 24 board crates (`impl BoardInit` provenance):

- **~22 crates implement the LEGACY family** (`nros-board-common::board_init`):
  every kernel board — native/posix, freertos + mps2-an385-freertos, threadx +
  threadx-linux + threadx-qemu-riscv64, nuttx (+ qemu-arm / qemu-riscv), zephyr,
  esp32-qemu/esp32s3, rtic-*, embassy-stm32f4, stm32f4, mps2-an385, orin-spe,
  bare-metal, cffi.
- **Only 4 (freertos, posix, threadx, zephyr) also touch
  `nros-platform::board`** — and they use it ALONGSIDE the legacy family, not
  instead of it.
- 2 newest (`fvp-aemv8r-smp`, `s32z270dc2-r52`) implement NEITHER.

So `nros-platform::board` (phase-212.N: config moved off `BoardInit` onto
`RuntimeCtx`) landed the TRAITS but essentially none of the board MIGRATION. The
legacy `nros-board-common::board_init` is the **de-facto board contract**; the new
family is aspirational. The shapes genuinely differ — legacy `BoardInit { type
Config; init_hardware(cfg) }` (the config-carrying `run<B: BoardInit>(cfg, f)`
kernel-family boundary) vs new `BoardInit { init_hardware() }` (config pulled from
`RuntimeCtx` inside `BoardEntry::run`). Finishing the migration means reworking
the generic-kernel `run` boot path + ~22 crates.

**This needs a maintainer design decision, not a mechanical fix (hence still
open):**
1. **Finish** — migrate the kernel-family boot path + all boards to
   `nros-platform::board`, delete `board_init` + the `NodeRuntime` alias. Big.
2. **Retire the new family** — if `RuntimeCtx`-based `BoardInit` isn't buying
   enough, delete the barely-used `nros-platform::board::{Board,BoardInit,
   BoardEntry}` duplicate and keep legacy as the single contract. Cheaper; drops
   the name collision.
3. **Keep both, document the split** — only if the two serve genuinely different
   layers permanently (the evidence says they don't — same names, superseding
   intent).

The `NodeRuntime` → `NodeDispatchRuntime` deprecated alias
(`nros-platform/src/board/mod.rs:86`) can be removed independently of the above
(its replacement exists; it's a one-release-cycle deprecation that has long
elapsed) — a cheap first step regardless of which direction (1/2/3) is chosen.
