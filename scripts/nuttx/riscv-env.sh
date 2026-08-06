#!/usr/bin/env bash
#
# The rv-virt (riscv32) values the shared NuttX board build.rs helpers read.
#
# # Why this file exists (issue 0456)
#
# `nuttx_image_link.rs` and `nuttx_ffi_build.rs` default every knob to
# **qemu-arm** — arch includes, linker script, board lib dir, and the
# vector-table head object. That default is deliberate (arm is the reference
# board), so a riscv build is only riscv if it SAYS so, six variables at a time.
#
# Three recipes provision the rv-virt kernel with the same preamble:
# `build-riscv-c`, `build-riscv-c-workspaces`, `build-riscv-rust`. Only the last
# one exported these. The two C lanes therefore inherited the ARM defaults, so
# phase-285 W4's `NUTTX_VECTORTAB=""` opt-out ("this arch has no vector-table
# head object") never happened and `run_image_link` archived
# `arch/arm/src/arm_vectortab.o` — left in the shared in-tree checkout by the
# previous ARM build — into a riscv image's boot archive. The link then reported
# it as MISSING (`cannot find -lnros_nuttx_boot`) rather than as the wrong arch,
# because `ld` skips an incompatible archive and then looks no further.
#
# Source it; do not copy it. A seventh variable added to one recipe and not the
# other two reproduces the bug exactly.
#
# Callers must have set `NUTTX_DIR` (or be about to). This file exports only
# arch-describing values, never the tree location, because the three recipes
# resolve that differently.

# Space-containing values (`NUTTX_PLATFORM_CFLAGS`, `NUTTX_ARCH_INCLUDES`) are
# why this is recipe/shell-level rather than per-fixture-row env in
# `examples/fixtures.toml`: the row serialization cannot carry them.
export NUTTX_CROSS="riscv-none-elf-gcc"
export NUTTX_PLATFORM_CFLAGS="-march=rv32imac -mabi=ilp32"
export NUTTX_ARCH_INCLUDES="arch/risc-v/src/chip arch/risc-v/src/common"
export NUTTX_LD_SCRIPT="boards/risc-v/qemu-rv/rv-virt/scripts/ld.script"
# EMPTY = "this arch has no vector-table head object". rv-virt's reset path
# lives in the kernel libs; only arm needs an object at the archive head.
# Phase-285 W4. Unset would mean the arm default, which is issue 0456.
export NUTTX_VECTORTAB=""
export NUTTX_VECTORTAB_OBJ=""
export NUTTX_BOARD_LIB_DIR="arch/risc-v/src/board"
