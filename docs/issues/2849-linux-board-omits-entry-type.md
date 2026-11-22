---
id: 2849
title: "The in-tree Linux board states no `[board.entry] type`, so an out-of-tree hosted board copied from it must set `NROS_BOARD_TYPE` by hand"
status: open
type: bug
severity: low
area: [boards, cli, macros]
related: [phase-589, issue-1285]
found: 2026-11-23
---
Found by a downstream proof of concept on an embedded Linux camera SoC (HiSilicon Hi3519DV500: 2x Cortex-A55, Linux 5.10, musl 1.2.3, BusyBox), built as an out-of-tree hosted board; see phase-589.

## What
`nros::main!` resolves the board type from `NROS_BOARD_TYPE`, else from the
hard-coded `BOARD_PATHS` table (`nros-orchestration-ir`, `board_path_for`).
A board descriptor CAN state it: `[board.entry] type` (`board_descriptor.rs`),
and `nros build` forwards it as `NROS_BOARD_TYPE` (`cmd/board_facts.rs`).

`packages/boards/linux/nros-board.toml` does not state it: the in-tree Linux
board is found through the table instead. An out-of-tree board written by
copying that descriptor inherits the omission, is in no table, and its build
fails until the user exports `NROS_BOARD_TYPE`, which is what the proof of
concept did.

## Fix direction
- Every in-tree board descriptor states `[board.entry] type`, so a copy
  carries it.
- The macro's error names `[board.entry] type` in the board descriptor as the
  fix, before the env variable.
- Consider whether the table is still needed once every descriptor states it.

## Acceptance
An out-of-tree board copied from the Linux descriptor builds with no
`NROS_BOARD_TYPE` in the environment.
