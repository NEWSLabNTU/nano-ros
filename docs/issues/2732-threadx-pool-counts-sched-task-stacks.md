---
id: 2732
title: "The derived ThreadX byte pool leaves out an image's sched-task stacks, because no compile of the pool sees how many there are"
status: open
type: enhancement
severity: low
area: [build, memory, threadx]
related: [2624, 2800]
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

## Scope (2026-11-22)

Planned before starting: about a day of work. It stays `low`, because the
one in-tree case boots at the default and the boot refusal names the knob.
The "Done when" above assumed the descriptor's `[[sched_task]]` rows already
carry what the pool needs. They do not: a row states the task's name, nodes,
subscription topics, parameter slots and `[image]` counts, but neither its
stack nor whether it is spawned. So there are three parts:

- **(a) The descriptor states each sched task's stack.**
  - A new per-row fact in `nros-sizing-descriptor` (`schema.rs`
    `SchedTask`): the stack in bytes, and whether the task is SPAWNED or is
    the boot task. The boot task runs on the app thread, whose stack the
    pool already counts.
  - Written by the model road, from the same lowering that gives the entry
    each sched task's `stack_bytes` (`codegen/entry/lower.rs`
    `member_stack_bytes`), so the descriptor and the entry cannot disagree.
  - A `stack_bytes` of 0 is a FLOOR the port raises, never 0 bytes. The fact
    must be the size the ThreadX port actually allocates: the same rule as
    the boot refusal's "requested" number, not the authored value.
  - A row nothing can state is refused per field, as every other descriptor
    fact is.
- **(b) Each road that compiles `threadx_hooks.c` reads it.**
  - Cargo, the riscv64 and threadx-linux board builds:
    `nros_board_common::threadx_sources::define_byte_pool` takes the total,
    read through `nros_sizing_descriptor::load_for_build_script` (which
    already watches the file).
  - CMake, rv-virt-threadx and threadx-linux: `cmake/NanoRosThreadxPool.cmake`,
    through `NanoRosSizingDescriptor.cmake`.
  - Both pass it as `-DNROS_THREADX_SCHED_TASK_STACK_BYTES`, the sum over
    SPAWNED tasks. With no descriptor (no contract, so no file), the value
    is 0 and the boot refusal stays the backstop. A refused field never
    defaults silently: it is 0 plus a build note naming the field.
- **(c) The pool counts it.**
  - In `threadx_hooks.c`, both the default and the `_Static_assert` floor
    become `app stack + sched-task stacks + WORKING_SET[rmw]`, so a STATED
    pool too small for the tasks fails at compile time with the knob, not at
    boot.
  - `threadx_pool::default_byte_pool_bytes` and `least_byte_pool_bytes`
    take the total, and their test holds the C file's derivation to them.
  - Acceptance: realtime-rust's `[image.threadx]` (one spawned 64 KiB task)
    derives a pool 64 KiB larger than today, and still boots. A test image
    whose tasks overflow today's default boots at the derived one.

**Host boards (issue 2800).** On threadx-linux every ThreadX thread runs on
a host pthread stack, so its pool-allocated stacks are dead weight there
([issue 2800](2800-threadx-linux-threads-run-on-host-pthread-stacks.md)).
Part (c) has to decide whether that board's derivation counts sched-task
stacks at all, or keeps the target's terms so host and target derive the
same pool.
