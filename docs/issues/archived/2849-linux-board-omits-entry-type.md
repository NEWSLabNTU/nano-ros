---
id: 2849
title: "The in-tree Linux board states no `[board.entry] type`, so an out-of-tree hosted board copied from it must set `NROS_BOARD_TYPE` by hand"
status: resolved
type: bug
severity: low
area: [boards, cli, macros]
related: [phase-589, issue-1285]
found: 2026-11-23
resolved: 2026-11-24
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

## Resolution
- Every shipped descriptor with a Rust entry states `[board.entry] type`
  (Linux, FreeRTOS POSIX and mps2, mps2 bare metal, ThreadX Linux and
  riscv64, both NuttX boards, every Zephyr board). The C-only boards
  (`mps3-an536-freertos`, both `s32z270-freertos`) have no Rust entry and
  state none. `board_key_table::every_rust_board_descriptor_states_the_tables_type`
  holds each to `BOARD_PATHS` (or, for the RTIC key, to its `entry_types`);
  it fails with the Linux `type` removed.
- A second gap appeared on the first acceptance run: with the type resolved,
  the entry did not compile (`cannot find nros_board_linux`), because the
  generated entry took its board crate from the table alone
  (`macro_board_crate`). `BoardFacts::from_descriptor_for` now takes it from
  the descriptor's `type` first, the macro's own precedence.
- The macro's two refusals (unknown board, retired `board =`) name
  `[board.entry] type` in `nros-board.toml` first; the crate-metadata route
  is mentioned second, for a plain cargo build.
- `nros board new` writes a commented `[board.entry] type` with the Linux
  hint, and the book's add-a-board page lists `type`.

Measured: a renamed copy of `packages/boards/linux/nros-board.toml`
(`names = ["my-linux"]`), reached through `NROS_EXTRA_BOARD_PATH`, builds the
Linux talker with `NROS_BOARD_TYPE` unset. The same copy without `type` fails
with the refusal naming `[board.entry] type`.

The table stays as the fallback for a build that does not go through
`nros build` (a plain cargo leaf, where nothing forwards the descriptor's
`type`); retiring it is not part of this fix.
