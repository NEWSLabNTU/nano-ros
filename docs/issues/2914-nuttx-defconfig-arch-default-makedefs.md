---
id: 2914
title: "build-nuttx.sh pairs any defconfig with the ARM board Make.defs unless NUTTX_BOARD_MAKEDEFS is also set, so a riscv defconfig alone compiles with host gcc and -mfloat-abi=soft"
status: open
type: bug
severity: low
area: [nuttx, build]
found: 2026-11-23
related: [2773, 2807]
---

**Owner:** unassigned.

## What happens

`scripts/nuttx/build-nuttx.sh` takes the board from two env values that are
independent of each other:

- `NUTTX_DEFCONFIG` — the board defconfig (default: the qemu-arm one).
- `NUTTX_BOARD_MAKEDEFS` — the board `Make.defs`, relative to `NUTTX_DIR`
  (default: `boards/arm/qemu/qemu-armv7a/scripts/Make.defs`).

The defaults are written twice (the `--menuconfig` arm and the main
`BOARD_MAKEDEFS=` line). A caller that sets only `NUTTX_DEFCONFIG` to the
riscv defconfig gets the riscv `.config` beside the ARM `Make.defs`. The ARM
`Toolchain.defs` it includes then reads `CROSSDEV` from the (riscv) config,
finds none, and compiles with `CC=gcc -mfloat-abi=soft`: the host compiler
refuses the flag, and the failure names neither file.

Seen 2026-11-23 in a cell recipe that set the riscv defconfig and not the
Make.defs. The in-tree `just/nuttx.just` riscv recipes set both, which is why
the lanes are green.

## Why it is the script's to fix

The defconfig already says which board it is: `CONFIG_ARCH` and
`CONFIG_ARCH_BOARD` (the script reads both a few lines later, to key the
export). Two inputs for one fact is a disagreement waiting for a caller.

## Fix

Derive the default Make.defs from the defconfig
(`boards/<CONFIG_ARCH>/<CONFIG_ARCH_CHIP>/<CONFIG_ARCH_BOARD>/scripts/Make.defs`,
or the board overlay's own `Make.defs`), keep `NUTTX_BOARD_MAKEDEFS` as an
override, and refuse an override whose path does not sit under
`boards/<CONFIG_ARCH>/`, naming both files. One computation, used by both call
sites. Acceptance: a run with only the riscv defconfig set configures the
rv-virt Make.defs; a mismatched override is refused before `configure.sh`.
