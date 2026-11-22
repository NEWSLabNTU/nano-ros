---
id: 2732
title: "The derived ThreadX byte pool leaves out an image's sched-task stacks, because no compile of the pool sees how many there are"
status: resolved
type: enhancement
severity: low
area: [build, memory, threadx]
related: [2624, 2800]
found: 2026-11-20
resolved: 2026-11-23
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

**Part (a) below is superseded by "Deferred" further down:** the descriptor's
`[[sched_task]]` rows exist only for Zephyr, so the facts are image-level.

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
([issue 2800](../2800-threadx-linux-threads-run-on-host-pthread-stacks.md)).
Part (c) has to decide whether that board's derivation counts sched-task
stacks at all, or keeps the target's terms so host and target derive the
same pool.

## Deferred (2026-11-21)

The revised scope (a) states two IMAGE-level facts, not a per-row one:
`spawned_sched_task_stack_bytes` and `spawned_sched_tasks_on_app_stack`.
`[[sched_task]]` rows exist only for a family with a per-task-sized runner
(Zephyr), and emitting them for ThreadX would also move ThreadX onto
per-task executor sizing. A spawned task that states no stack gets the app
thread's (`nros_threadx_create_task`), so the board computes
`sum + count * app stack`. When a fact is absent, the build prints that
sched-task stacks are not counted in the pool on that road. threadx-linux
counts them too: that is the conservative choice, although its threads run
on host pthread stacks and so never use them
([issue 2800](../2800-threadx-linux-threads-run-on-host-pthread-stacks.md)).

**Why it is deferred.** The facts' producer has to be the SAME table the
entry runs. On the Rust road, `nros::main!` builds that table inside the
proc macro (`packages/core/nros-macros/src/main_macro.rs`): resolution,
derived placements, `derive_level_stacks` (member stacks plus the port's
worker frame), spin periods and mode deadlines. That runs at app compile
time, after the board's build script has already compiled
`threadx_hooks.c`.

**The plan when it is picked up.** Extract that pipeline into one
`nros-orchestration-ir` function (model, RTOS and overlay in; the resolved
sched-task table and its notes out). The macro calls it, keeping its
diagnostics, and the descriptor's model road calls it at `nros configure`; the
two facts are read off the returned table. Estimated at 1-2 days.

**Rejected:** a second, simpler derivation in the CLI (authored
`stack_bytes` only). It would miss stacks derived from a level's members,
and so UNDER-count the pool, which is the unsafe direction.

## Resolved (2026-11-23)

Built to the revised scope, with the producer question answered differently
from the plan above: no pipeline was extracted. Instead the CLI calls the
macro's own stack rule, and the macro checks the result.

- **One rule, two callers.** `nros build` resolves the entry's table as
  before (`resolve_plan_sched`) and derives each level's stack with
  `nros_orchestration_ir::derive_level_stacks_for`. `nros::main!`'s
  `derive_level_stacks` is that function's Rust-runner case, so the two cannot
  hold different rules. `BoardFamily::rust_spawned_task_stacks`
  (`nros-entry-lower`) decides which tasks are spawned, for both.
- **Two image facts** (`[image] spawned_sched_task_stack_bytes`,
  `spawned_sched_tasks_on_app_stack`), stated for a Rust ThreadX entry.
- **The macro guards the inputs.** `nros::main!` reads the descriptor its
  build was named and refuses an image whose own table spawns other stacks,
  naming both numbers. Since the rule is shared, a refusal means the inputs
  differ: a stale descriptor, or a model the build did not resolve.
- **The pool:** `app stack + sched-task stacks + WORKING_SET[rmw]`, in both
  `threadx_pool::{default,least}_byte_pool_bytes` and the C file's default
  and `_Static_assert`. A spawned task with no stack costs one app stack
  (`threadx_pool::sched_task_stack_bytes`). A test holds the C file to it.
- **No descriptor:** the cargo road counts 0 and prints a build warning. The
  warning says the boot then refuses a task that does not fit, and that
  `nros build` states the tasks. The boot refusal now says the default counts
  only the stated tasks.
- **The cmake road** builds C/C++ entries, whose ThreadX runner spawns no
  sched task, so its answer is 0 by construction, and nothing reads the facts
  there.
- **threadx-linux** counts them too: the conservative choice (issue 2800).

Measured, realtime-rust `[image.threadx]` (threadx-linux, one spawned task):

| case | pool | boot |
|---|---:|---|
| as authored (no task stack), before | 459,008 B | boots |
| as authored, after | 524,544 B (+65,536, one app stack) | both levels run |
| `high` with 384 KiB, before (descriptor without the two facts) | 459,008 B | `FATAL ... 393216 bytes requested, 277568 free of 459008` |
| `high` with 384 KiB, after | 852,224 B | both levels run; `nros_level` used 33,080 of 393,216 |
| the same, pool stated at 459,008 B | (build) | refused: "below the 577632 this image needs (... 262144 bytes for its spawned sched tasks' stacks ...)" |

(The last row ran at a 256 KiB stack.) A 256 KiB task still fit the old
pool (277,568 B free at spawn), so the overflow case needs one larger than
that.

The boot refusal, the backstop for a stack no descriptor states, is a test
now: `tests/sched_task_pool_refusal_threadx_linux.rs`, over an owned-entry
fixture that spawns a 16 MiB task (`FATAL ... 16777216 bytes requested,
344288 free of 409856`).

Found while measuring, filed separately: issue 2859 (`[knobs.memory]
heap_bytes` has no rebuild edge into the pool) and issue 2860 (the
`[knobs.threadx] byte_pool_bytes` spelling every message names is refused
for a threadx-linux image).
