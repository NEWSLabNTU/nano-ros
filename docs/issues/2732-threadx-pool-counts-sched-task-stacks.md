---
id: 2732
title: "The derived ThreadX byte pool leaves out an image's sched-task stacks, because no compile of the pool sees how many there are"
status: open
type: enhancement
severity: low
area: [build, memory, threadx]
related: [2624]
found: 2026-11-20
---

## What

Issue 2624 replaced the ThreadX byte pool's 4 MiB literal with a derived size:

```text
app stack + WORKING_SET[rmw]
```

A sched task's stack also comes out of that pool, and the default does not
count it. The pool is sized in `nros-board-common/c/threadx_hooks.c`, and that
file is compiled by the BOARD build on four roads: the cargo riscv64 and
threadx-linux glue, and the cmake rv-virt-threadx and threadx-linux board
files. None of them sees the image's sched tasks. The entry's
`NROS_ENTRY_SCHED_TASK_COUNT` is in the generated entry header, and the sizing
descriptor's `[[sched_task]]` rows are read by no board build.

So an image's sched tasks have to fit in the working set's margin, or the image
states its pool. The margin is sized from the RISC-V board, whose NetX allocates
from the pool, so on threadx-linux, which runs no NetX, it is generous. The one
in-tree ThreadX image with sched tasks (realtime-rust's `[image.threadx]`: one
spawned 64 KiB task besides the boot task) runs at the default. An image that
does not fit stops at boot. The stack allocation is fatal, with the knob, the
requested bytes and the pool's free bytes:

```text
FATAL: the ThreadX byte pool cannot hold a sched task's stack: 262144 bytes
requested, 211992 free of 393216. Raise `[knobs.memory] heap_bytes`
(NROS_THREADX_BYTE_POOL_KB) by at least the shortfall; ...
```

## Done when

- Each road that compiles `threadx_hooks.c` hands it the image's spawned
  sched tasks' stack total, read from the sizing descriptor (the one carrier of
  per-image facts, RFC-0100), and the default adds it:
  `app stack + sched-task stacks + WORKING_SET[rmw]`.
- `nros_board_common::threadx_pool::default_byte_pool_bytes` takes the total,
  and its test holds the C file's derivation to it.
- The boot refusal stays as the backstop for a stack the build could not see.
