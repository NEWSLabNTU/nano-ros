---
id: 2624
title: "The ThreadX byte pool is a 4 MiB literal on both build roads, not a derived size"
status: open
type: performance
area: [build, memory, threadx]
severity: low
found: 2026-11-16
related: [2623, 1145, 1171, 1388]
---

**Owner:** unassigned. **Home:** follow-up to issue 2623 (no phase yet).

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
