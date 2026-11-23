---
id: 2865
title: "FreeRTOS resolves the wall clock on its first read, which runs before the network is up"
status: resolved
type: bug
severity: medium
area: [freertos, platform]
related: [rfc-0141, phase-582, phase-478]
found: 2026-11-23
resolved: 2026-11-23
resolved_in: "fix(freertos): resolve the wall clock at a boot point after the network is up (issue 2865)"
---
On a bare-metal FreeRTOS image the port resolved the `[knobs.clock] source`
list on the first `nros_platform_time_now_ns` read from a task. That read is
not the application's. With Cyclone DDS it is a static constructor:
`ddsrt_ctor` → `ddsrt_random_init` → `dds_time` → `clock_gettime(REALTIME)`.
The board entry runs it in the app task through
`nros_board_freertos_run_init_array()`, and that call comes before
`nros_freertos_init_network`. Both entries do this: the Rust
`freertos_boot_bringup` in `nros-board-freertos/src/entry.rs`, and the C/C++
lane's `freertos_c_entry.c`.

So:

- the settle window (`clock.fallback_settle_ms`) stalled the task that brings
  the network up;
- a `board` hook backed by an lwIP SNTP task could never answer inside it,
  and the resolution latched "unanswered";
- the verdict line (`nros: wall clock from …` / `no wall-clock source
  answered`) was reported before the board registered the port's log writer,
  so nothing printed it.

Seen by a consumer on mps3-an536 with Cyclone. s32z270 has the same entry
shape.

## Fix

The FreeRTOS port resolves at an explicit boot point,
`nros_platform_freertos_clock_resolve_point()` (`nros/platform_freertos.h`,
outside the bindgen surface like `platform_zephyr.h`). It runs the resolution
once, with the same semantics as before: the settle-window poll for `board`,
then the fallback list, then `epoch_failure`. It latches the result and
reports the verdict through the port's log writer. Before the point, reads
resolve nothing and spend no window: `nros_platform_time_now_ns` reads 0, and
`nros_platform_wall_clock_state` is "not synced", naming the first listed
source. Called before the scheduler runs, the point does nothing. An image
with no list gets a no-op point and keeps its default `platform` behaviour:
0 on bare metal, the host's `CLOCK_REALTIME` on freertos-posix.

Every FreeRTOS entry calls the point from the app task after network
bring-up and log-writer registration:

- `nros-board-freertos/c/freertos_c_entry.c`, after
  `nros_platform_register_log_writer`, which itself follows network init and
  the poll task. This covers every C/C++ FreeRTOS board: mps2-an385,
  mps3-an536, s32z270 and the out-of-tree r5f fixture.
- `nros-board-freertos-posix/c/freertos_posix_entry.c`, after its log-writer
  registration. The host owns the network.
- `nros-board-freertos/src/entry.rs`, at the end of `freertos_boot_bringup`:
  after `init_network`, the lwIP drain task and the netif check on bare
  metal, and at the same place on the POSIX port. Every Rust entry flavour
  (`run_entry`, `run_sched_tasks_entry`, `run_bare`) goes through it, and the
  board crates register the port's log writer before they call into it.

POSIX keeps its lazy `pthread_once`. A host process has its network before
`main`, so it has no such ordering problem.

Test: `tests/clock-board-freertos-tests.sh` (`just check
clock-board-freertos`), on the real kernel on its POSIX port, has two new
cases:

- a hook that answers only once a "network up" flag is set;
- an early read, then longer than the window, then the point.

Every list case first checks that a read before the point reads 0, not
synced, and asks the hook nothing. Two negative controls: an inert point
fails `board-late`, and a lazy-on-first-read port fails both new cases.

## Resolved, 2026-11-23

Fixed in a10b403187 (PR #4639): FreeRTOS resolves the clock at
`nros_platform_freertos_clock_resolve_point()`, after network bring-up, in
every C, C++ and Rust entry and on freertos-posix. Tests: `just check
clock-board-freertos` (board-late and early-read cases, with negative
controls). Confirmed done by the phase-582 owner. Consumer evidence: the
island's AN536 image logs "nros: wall clock from board" (its CI run
45075448610). The "clock resolution took N ms" log line is a phase-582
follow-up, not part of this issue.
