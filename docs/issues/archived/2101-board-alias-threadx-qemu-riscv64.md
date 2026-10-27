---
id: 2101
title: "Board alias `threadx-qemu-riscv64` names the emulator, not the board"
status: resolved
type: tech-debt
area: [boards, naming]
severity: low
found: 2026-10-27
resolved: 2026-10-27
related: [RFC-0093, issue-2063, phase-437, phase-445]
---

## What

The bare-metal ThreadX board on QEMU's RISC-V `virt` machine answered to
`threadx-qemu-riscv64`, a word that names the emulator (RFC-0093 R3: a board
name never names how the image is run). RFC-0093 §4 had already chosen
`rv-virt-threadx` — the index key, the cmake module and the example directory
use it, and since phase-445 W3b every in-tree leaf's `system.toml` authors it.
The old word survived as a descriptor alias, a `<nano_ros_provides>` export,
a row in `nros-orchestration-ir::BOARD_PATHS` and in
`nros_entry_lower::BOARD_KEYS`, so `nros::main!`, `nros setup` and the board
catalog all still accepted it, and `check-board-name-reach` carried it in its
baseline (`alias:threadx-qemu-riscv64`).

Issue 2063 retired `qemu-mps2-an385` the same way and left this one to be
ruled on its own facts. Those facts are the same: it names the emulator, it
was never an index key, and nothing authored needs it.

## Fix

Same recipe as issue 2063:

- The descriptor's `names` (`packages/boards/nros-board-threadx-qemu-riscv64/nros-board.toml`)
  and the `package.xml` exports list `rv-virt-threadx` first and no longer
  carry the old word.
- The `BOARD_PATHS` and `BOARD_KEYS` rows are gone. The old word is a row in
  `nros_entry_lower::RETIRED_BOARD_KEYS`, so the macro, the typed entry and
  the board catalog (`nros setup`) refuse it with
  ``board `threadx-qemu-riscv64` is retired: write `rv-virt-threadx`.`` It is
  never mapped silently.
- It is removed from `.config/board-name-reach-baseline.txt`.
- Recorded in the retired-board-names ledger, the book's `retired-names.md`,
  an RFC-0093 amendment, and `changelog.d/2101.breaking.md`.
- Prose and comments that used the old word as the board's name now say
  `rv-virt-threadx`.

The board crate keeps its directory and package name,
`nros-board-threadx-qemu-riscv64` (`ThreadxQemuRiscv64`), because RFC-0093
moved no crate directories.

## Evidence

- Refusal tests: `nros-entry-lower`
  `the_emulator_named_threadx_riscv_key_is_refused_naming_the_board`,
  `nros-macros` `leaf_axis_tests::the_emulator_named_threadx_riscv_key_is_refused_naming_the_board`,
  and the catalog assertion in `board_descriptor::tests::an_image_board_alias_resolves_to_its_setup_key`.
- Mutation-checked. Restoring the `BOARD_KEYS` and `BOARD_PATHS` rows (a
  silent alias) turns the macro and entry-lower tests red. Deleting the
  `RETIRED_BOARD_KEYS` row turns all three red.
