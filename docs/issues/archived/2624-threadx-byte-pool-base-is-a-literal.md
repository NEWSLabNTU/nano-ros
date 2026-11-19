---
id: 2624
title: "The ThreadX byte pool is a 4 MiB literal on both build roads, not a derived size"
status: resolved
resolved_in: "the change that archives this issue"
type: performance
area: [build, memory, threadx]
severity: low
found: 2026-11-16
related: [2623, 1145, 1171, 1388, 2732]
---

**Owner:** nros 243 (2026-11-19). **Home:** follow-up to issue 2623 (no phase yet).

## What

`BYTE_POOL_BASE_SIZE` in `packages/boards/nros-board-common/c/threadx_hooks.c`
is `4 * 1024 * 1024`. The ThreadX byte pool is the whole allocator on this
port. It hands out the boot thread's stack, every sched task stack, zenoh-pico's
buffers and nano-ros's own allocations, plus NetX Duo's packet pool and stacks
on the boards that run NetX. Nothing derives the 4 MiB. Only the subtrahend
paired against the `.bss` executor backing is derived (issue 1171).

This is the same shape as issue 2623's option 2, where the FreeRTOS cmake
road's heap was a fixed 3 MiB literal and is now derived. ThreadX differs in
one way: the literal is the same on the cargo and the cmake road, so no road
is better sized than the other.

## Measured

The only peaks on record come from the pool reporter that issue 1145 added
(`nros-board-threadx/src/entry.rs`, "ThreadX byte-pool peak reporter").
They were taken on threadx-linux, x86_64, against a live `rmw_zenohd`, on
2026-09-18:

| image | pool peak | of |
| --- | --- | --- |
| talker | 181,144 B | 4,194,304 B (4.3 %) |
| listener | 180,824 B | 4,194,304 B (4.3 %) |

These numbers do not size the base, for the reasons `entry.rs` gives:

- They were taken with the executor backing drawn FROM the pool
  (`backing_u64s = 0`, issue 1388), so a build that uses the static backing
  moves those bytes out.
- They cover two of the six roles. The service client and action server failed
  to connect before the first sample.
- threadx-linux runs no NetX setup (`nros_board_init_eth` is a no-op there),
  so the pool's largest other consumer allocates nothing. The RISC-V board
  (`threadx-qemu-riscv64`) runs the full NetX setup, and it is where the base
  has to be judged. No peak has been recorded there.

## Next step

Measure the pool peak on `threadx-qemu-riscv64` for each role, on both roads,
with the static executor backing. Then derive the base the way
`nros_board_common::freertos_config::default_heap_bytes` derives the FreeRTOS
heap: from measured terms with a stated margin. Hold the C literal to that
derivation with a unit test. Do not change the base until it has been measured
on that board.

## Resolved (2026-11-20)

Measured on `threadx-qemu-riscv64`, Rust, cmake road, static executor backing,
each role paired over QEMU. The zenoh rows were already on record; Cyclone's are
new, now that issue 1590 links a Rust Cyclone image there.

| image (Cyclone) | pool peak |
| --- | --- |
| talker | 1,047,096 B |
| listener | 1,042,472 B |
| service-server | 1,059,840 B |
| service-client | 1,123,696 B |
| action-server | 1,136,992 B |
| action-client | 1,317,840 B |

The action-client figure comes from runs that completed their goal (`Result
received` logged before the peak). Its `TIMEOUT: no completion` line on some
runs is the first goal timing out and the example retrying, which then
completes, so it does not lower the peak.

The pool is now `app stack + WORKING_SET[rmw]`, per RMW
(`nros_board_common::threadx_pool`). Each working set is the worst peak less the
512 KiB app stack, with a stated margin: zenoh 327,680 (1.63x 200,800), Cyclone
1,179,648 (1.49x 793,552). On RISC-V that is 832 KiB for a zenoh image and
1,664 KiB for a Cyclone one, instead of 4 MiB. The Cyclone roles re-run in it
complete, with a worst peak of 1,316,696 and 387,240 B free.

- The knob is `[knobs.memory] heap_bytes`, the ladder's existing "RTOS heap"
  key, with the ThreadX front-end `NROS_THREADX_BYTE_POOL_KB`. It reaches
  every compile of `threadx_hooks.c`: `threadx_sources::define_byte_pool` on
  the cargo road, `cmake/NanoRosThreadxPool.cmake` on the cmake road.
- A stated pool below the app stack plus the measured working set is refused
  at build time, naming the knob (the board build, and the C file's
  `_Static_assert`).
- The pool no longer subtracts the executor backing. The peaks were measured
  with the backing in `.bss`. `check-executor-backing-arena-pairing`'s
  ThreadX arm now refuses the subtraction.
- `threadx_hooks.c` holds the measured numbers as literals, and
  `the_c_file_states_the_derived_working_sets` holds them to the Rust.

An image's sched-task stacks are not in the default, because no compile of the
pool sees how many there are (issue 2732). A stack that does not fit is fatal at
boot, naming the knob, the requested bytes and the free bytes. Measured on
threadx-linux by giving realtime-rust's `high` task a 256 KiB stack:

```text
FATAL: the ThreadX byte pool cannot hold a sched task's stack: 262144 bytes
requested, 211992 free of 393216. Raise `[knobs.memory] heap_bytes`
(NROS_THREADX_BYTE_POOL_KB) by at least the shortfall; an image's sched tasks
are not in the derived default.
```

The image exits 1 and the task never starts. At its own stack sizes
realtime-rust runs at the default: both sched tasks come up and publish.
