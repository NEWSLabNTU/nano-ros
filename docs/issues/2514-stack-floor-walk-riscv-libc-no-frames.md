---
id: 2514
title: "The stack walk refuses every rv-virt-threadx cargo image: the board's bundled libc objects carry no frame and no CFI"
status: open
type: limitation
severity: low
area: [memory, build, threadx]
related: [issue-2400, phase-557]
found: 2026-11-14
---

## What

Phase-557 W5b makes the `rv-virt-threadx` board state its app-thread stack
(`NROS_THREADX_APP_STACK_KB`, written to `nros-stack-stated.json` with root
`~app_task_entry_`). The cargo road's walk then could check it, as it does on
FreeRTOS and bare metal. But it cannot run on this board, so `threadx-riscv64`
is not in `nros build`'s `STATING_PLATFORMS` yet.

The board's rlib carries the toolchain's `libc.a` members
(`libc_a-memchr.o`, `libc_a-vsnprintf.o`, …). They have no `.su`, and the
linked image's `.debug_frame` and `.eh_frame` have no FDE for them. The
FreeRTOS road takes newlib's frames from the linked image's CFI (W5), and that
source is empty here.

## Measured

`packages/testing/nros-tests/bins/param-store-threadx-riscv64`,
`riscv64gc-unknown-none-elf`, `dev` profile, xPack `riscv-none-elf-gcc` 14.2,
walked from `~app_task_entry_` with the platform switched on locally:

```
nros-stack-floor: 0 frame(s) read from …/rv_virt_threadx_entry's .debug_frame (functions no input states one for)
nros-stack-floor: REFUSED
root `…create_session_trampoline…`:
  - functions reached with no frame in any input (a missing frame would count as 0): __bswapsi2, memchr, strcmp, strncmp, strncpy, strtoul, vsnprintf
```

`llvm-objdump --dwarf=frames` lists 5,980 FDEs and none for `memchr`
(0x8001c018), `vsnprintf` (0x800396b6) or `_svfprintf_r` (0x800171ec).
The CFA register is not the cause: the parser's `reg13` (Arm SP) and `reg2`
(RISC-V SP) give the same 994 frames.

There may be a second gap behind the first. Not measured: `vsnprintf` calls
`_svfprintf_r` (`jal 800171ec` at 0x800396f4), which has no frame either, yet
it is not in the missing list. Either the walk does not go past a frameless
function, or that edge is not on the graph. If it is the edge, stating frames
for the seven functions alone would undercount the printf chain. Settle this
before stating any frame.

## Impact

The ThreadX riscv64 cargo image is reported as not checked, as before W5b.
Nothing is enforced and nothing is wrong. The stated file is written and
nothing reads it yet.

## Fix

The walk needs frames and edges for libc objects that have neither `.su` nor
CFI. Two ways:

1. Read a frame from the disassembly of such a function: its prologue's
   `addi sp,sp,-N` on RISC-V (`sub sp`/`push` on Arm). Refuse a function
   whose SP moves by a register (alloca). Take the edges from the linked
   image's disassembly rather than from the object's relocations.
2. Build the board's libc with `-fstack-usage` and frames, as the C sources
   the board compiles already are.

When it walks, add `threadx-riscv64` to `STATING_PLATFORMS` in
`packages/cli/nros-cli-core/src/cmd/stack_floor.rs`.
