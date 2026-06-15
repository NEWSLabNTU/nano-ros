---
id: 69
title: dep-chain gate red — stm32f4 / qemu-arm-baremetal talkers dropped the rmw-zenoh feature
status: resolved
type: bug
area: ci
related: [phase-244]
resolved_in: "scripts/ci/dep-chain-check.sh — own-feature detect + package.xml-gated codegen"
---

> **RESOLVED (2026-06-16).** Two bugs in `scripts/ci/dep-chain-check.sh`, both
> hit by the board-driven baremetal talkers (post-C6 they carry NO `rmw-*`
> feature — the board crate selects the backend):
> 1. The per-cell feature detect was a substring grep over the whole
>    `cargo metadata` JSON, which also matched a DEPENDENCY's requested features
>    (`nros-board-stm32f4 { features=["rmw-zenoh"] }`) → it wrongly passed
>    `--features rmw-zenoh` to an example whose OWN feature table is empty →
>    "does not contain this feature: rmw-zenoh". Fixed: check the package's own
>    `features` table via `python3` (`json.load(...)['packages'][0]['features']`).
> 2. Step-2 codegen ran `nros generate-rust` unconditionally, but those talkers
>    ship no `package.xml` (no generated interfaces) → "Failed to read
>    package.xml". Fixed: skip codegen when `$ex/package.xml` is absent.
>
> Validated: `just check-dep-chain` → **9 passed, 0 failed (of 9 cells)**.

## Symptom

`scripts/ci/dep-chain-check.sh` (the `check.yml` per-(board,rmw) resolution gate,
now also `just check-dep-chain`) fails 2 of 9 cells:

```
error: the package 'stm32f4-bsp-talker' does not contain this feature: rmw-zenoh
dep-chain: 7 passed, 2 failed (of 9 cells)
  FAILED: qemu-arm-baremetal:zenoh
  FAILED: stm32f4:zenoh
```

This is a standing red on `check.yml` (the fast gate has been failing on main).

## Cause

`examples/stm32f4/rust/talker` (and the qemu-arm-baremetal talker) carry **no
`[features]` block / no `rmw-zenoh`** — the C6-tail refactor
(`b29e69604` / `0efc908b8`, phase-244/248) stripped the `rmw-*` selector features
from those baremetal talker examples, but `dep-chain-check.sh`'s `CELLS` matrix
still probes `<board>:zenoh` with `--features rmw-zenoh`.

## Fix direction (not started)

Either:
1. Restore the `rmw-zenoh` (and siblings) feature on the baremetal talker examples
   so the cell resolves, **or**
2. Update `dep-chain-check.sh`'s cell matrix / per-cell feature logic to match the
   examples' current feature shape (board-driven selection, no `rmw-*` feature on
   the example).

Pick whichever matches the post-C6 example contract (board crate selects the RMW;
the example likely should NOT carry `rmw-*` → option 2). Surfaced by the CI reorg
(`just check` SSoT); see `docs/development/ci-workflow-reorg.md`.
