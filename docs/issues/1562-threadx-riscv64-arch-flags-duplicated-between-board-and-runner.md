---
id: 1562
title: "The ThreadX RISC-V64 ISA/ABI flags and the picolibc sysroot probe exist
  twice — in the family runner and in the base board's own build script"
status: open
type: tech-debt
area: boards, build
severity: low
related: [1561, phase-471]
---

## What this is

`-march=rv64gc -mabi=lp64d -mcmodel=medany -fno-builtin` is written out in two
places, and so is the `--specs=picolibc.specs -print-sysroot` probe beside it:

| site | what it holds |
| --- | --- |
| `packages/boards/nros-board-common/src/threadx_qemu_riscv64_build.rs` | `configure_riscv64` (`:350`), `get_picolibc_sysroot` (`:398`), the newlib probe (`:458`) |
| `packages/boards/nros-board-threadx/build.rs` | the same four flags (`:339`), a second `get_picolibc_sysroot` (`:360`) |

Both reach `nros_build_paths::riscv64::tool_or_legacy("gcc")` for the compiler,
so the TOOL has one spelling; the FLAGS it is handed do not. This is the same
shape phase-471 W2 removed from the FreeRTOS family, where three board scripts
each carried a private `gcc_print_file` with its own hardcoded `-mcpu` list —
and where the two halves had already drifted, because `FREERTOS_CFLAGS` won for
the compile and could not reach the multilib probe.

## Why it is filed rather than fixed

`check-board-build-wiring` gained the rule that catches this class in
phase-471 W2 — a board `build.rs` may not spell an ISA/ABI flag, because the
family builder in `nros-board-common` is what makes that one answer. The
ThreadX site carries a `// nros-board-arch-flags-exempt:` naming this issue
rather than being changed in the same commit, for two reasons:

* phase-471 W6 owns `nros-board-threadx/build.rs`'s include lists right now,
  and the flags sit fourteen lines above them;
* acceptance for this family is a **RISC-V64 ThreadX build**, not a gate —
  moving the flags without one would be exactly the change phase-471 W2 refused
  to make for the FreeRTOS overlays.

## What the fix looks like

The FreeRTOS answer transfers: one `configure_riscv64` in
`nros_board_common::threadx_*`, called by both, and ONE picolibc/newlib probe
that takes its flags from the same function the compile does — not a second
hardcoded copy of the list. The ThreadX family has no `[arch.*]` profile the
way FreeRTOS does, so the flags stay a Rust constant; what changes is that
there is one of it.

Do not fix it with a shared constant alone: the FreeRTOS drift was between the
COMPILE flags and the LIBRARY-PROBE flags, and a constant that both copy is
still two call sites that can diverge in what else they pass.
