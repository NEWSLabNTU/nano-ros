---
id: 2709
title: "Two older carriers still restate an arch profile's C flags beside issue 2692's one carrier"
status: open
type: tech-debt
severity: low
area: [build, toolchain]
related: [issue-2692, phase-575]
found: 2026-11-18
---

## What remains

[Issue 2692](archived/2692-cc-rs-builds-skip-arch-profile.md) made the image's
`[arch.*]` profile the one source of `CFLAGS_<triple>` / `CXXFLAGS_<triple>`
for every cc-rs build, written `force`d into the cargo config of every road.
Two older spellings of the same flags were kept:

1. `cmake/NanoRosCargoProfile.cmake`: `nros_arm_r_fpu_cflags_env` (Cortex-R5F
   and R52) and the CFLAGS half of `nros_riscv64_rustflags_env`. They set the
   flags through Corrosion's build-time environment, keyed on
   `CMAKE_SYSTEM_PROCESSOR`, with the values copied by hand from the profiles.
2. `packages/boards/nros-board-threadx-qemu-riscv64/nros-board.toml`: an `[env]`
   row `CFLAGS_riscv64gc_unknown_none_elf`, the `[arch.riscv64gc]` profile's
   value.

Both agree with the profile today. Where both apply, the forced carrier row
wins. They are a second and third place to change when a profile changes, and
a disagreement would be silent.

## Why they were not deleted with 2692

The cmake functions also run for cmake configures that may not reach the
image-env file (a configure with no `system.toml` image). No lane has
measured that every such road gets the carrier's row. Deleting them unmeasured
could leave a hard-float Cortex-R or RISC-V C build with cc-rs's defaults
(gcc: "selected architecture lacks an FPU"; lld: "cannot link object files
with different floating-point ABI").

## Fix

Delete both, and keep `nros_riscv64_rustflags_env`'s `RUSTFLAGS` half, which
is not a C flag. Do it once a lane builds every cmake Cortex-R and RISC-V road
(the `mps3-an536` FreeRTOS fixtures, `threadx-riscv64`, the out-of-tree R5F
board) and the archive-member check from 2692
(`just check out-of-tree-board-images`) passes over them. Retire the
toolchain files' "read by `nros_arm_r_fpu_cflags_env`" comments with them.
