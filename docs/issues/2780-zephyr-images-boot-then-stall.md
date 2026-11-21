---
id: 2780
title: "merge-runtime zephyr shard, 2cfe9f0094..b13a5d4b18: two Zephyr realtime rows stall (unconfirmed solo); the shim cell is load"
status: open
type: bug
area: [zephyr, ci, testing, params]
severity: high
found: 2026-11-21
related: [2777, 1575, 1551, 2782]
---

## What fails

`merge-runtime.yml` run 44768834808 (lane `merge`, main at `b13a5d4b18`), the
zephyr shard (job 136400725703, runner newslab-server-241), has three real
failures. Each is a Zephyr image that boots and then does not get on with
its work:

1. `cli_bringup_zephyr::cli_bringup_zephyr_adapter_shim_boots_native_sim`:
   "native_sim ELF did not print the baked-config boot line — shim contract
   broken", with EMPTY output.
2. `rmw_coordinate_truth::a_rows_entry_registers_its_nodes_at_runtime`:
   `workspace-probe-seeds (zenoh) — did not reach node registration`. The image
   printed its boot config (domain 13, the test's locator, rmw zenoh) and
   nothing more.
3. `realtime_sched_tasks_e2e::realtime_sched_tasks`: 2 of 2 rows failed.
   - `zephyr/cpp-derived`: node `mrm_emergency_stop_operator` never ticked
     twice, so its derived sched task did not run (issue 1575's image, the one
     issue 1551 ran the heap dry on).
   - `zephyr/cpp-levels`: "too few complete callbacks to judge (0 middle,
     1 lazy)".

## A regression

| run | main | zephyr shard |
| --- | --- | --- |
| 44090176782 | 2cfe9f0094 (2026-11-17) | all three PASS |
| 44721090137 | 0d9429d990 (2026-11-20) | stopped at build (issue 2767), no verdict |
| 44768834808 | b13a5d4b18 (2026-11-20) | all three FAIL |

Read from each run's `merge-runtime-merge-zephyr-junit` artifact.

## Where to look

This is the window of issue 2777 (threadx_linux: an image boots and never
completes its session open), and the symptom has the same shape: an image that
boots and then starves or stalls. A shared root is possible, so the bisect for
2777 should include one of these cells; `cli_bringup` is the cheapest (a single
boot line on native_sim). Suspects, by subject:

1. `6229085857` every image serves the stock node interfaces by default: more
   entities, so more heap and stack at start, which is how issue 1551's image
   failed before.
2. `67d36cb764` a board's executor backing rung is a floor, not a ceiling.
3. `1a896a6722` durations read the monotonic clock (issue 2760).

## Closes when (superseded)

As filed: all three pass again with the cause named. Superseded by the
narrowed condition at the end.

## Narrowed, 2026-11-21

Each of the three was run again on its own, outside the merge lane.

### 1. `cli_bringup_zephyr_adapter_shim_boots_native_sim`: not a regression

It passes 3 of 3 solo at `b13a5d4b18` (nros 142, host load about 20, fixture
freshly built). The empty output on the merge lane fits host load against the
cell's 2 s boot window. That window is load-sensitive, and that is the only
thing this issue keeps of it.

### 2. `a_rows_entry_registers_its_nodes_at_runtime`: real, a Linux image, filed as issue 2782

`workspace-probe-seeds` is a LINUX row (cmake, mixed C/C++); it was in the
zephyr shard's junit, which is why this issue first called it a Zephyr image.
It reproduces 3 of 3 solo: its C component's 41 declared parameters are
refused by an 8-slot parameter store, and the image exits `-6` with nothing
printed. That is a different class from a stalled Zephyr boot, so it is
tracked on its own as issue 2782 and leaves this issue. Issue 2782 turned out
to duplicate issue 2768, already fixed by `9fecdfd146`; the cell passes again at a
main that contains it.

### 3. `realtime_sched_tasks` (`zephyr/cpp-derived`, `zephyr/cpp-levels`): real, the platform heap runs out

Both rows reproduce 3 of 3 solo on newslab-241 at main `3b6959ca7f`, in 21 s,
2 s and 2 s. Neither is a scheduling defect: each image panics on the Zephyr
platform heap within half a second of boot, before its sched tasks have run
long enough to judge.

| row | last lines before the panic |
| --- | --- |
| `zephyr/cpp-derived` | `scheduler core=0 levels=2 backing=232`; `parameter store: 8 slots (70848 B) + state and services (36480 B) carved from the executor backing`; the stock parameter services of 4 nodes |
| `zephyr/cpp-levels` | `levels=3 backing=336`; `parameter store: 6 slots (2160 B) + state and services (15544 B)`; `sched task entered`; `sched task stack sched_task=derived-middle bytes=16384`; `priority set sched_task=derived-lazy prio=7` |

Both end the same way:

    HEAP EXHAUSTED (TOO SMALL): request 96 bytes, arena 115200 bytes, free 48 bytes, largest free block 48 bytes
    PANIC platform heap exhausted @ packages/platform/nros-platform-zephyr/src/platform.c:1736

So the 114,688 B `CONFIG_NROS_ZEPHYR_HEAP_SIZE` arena (115,200 with the
slab beside it) no longer fits these two multi-node C++ images.
`CONFIG_NROS_HEAP_EXHAUSTION_IS_FATAL=y` in both.

**The allocation that tips it.** Fresh rebuilds at `3b6959ca7f`
reproduce the same callers, `0x428269` (derived) and `0x428895` (levels). Both
resolve to `_z_write_filter_create` (`zenoh-pico/src/net/filtering.c:242`),
zenoh-pico's 96-byte per-publisher write-filter context. That is only the
allocation that comes LAST. The heap is already full of the stock interfaces'
endpoints.

**Why the heap is too small: two commits in the window.**

1. `6229085857` (every image serves the stock node interfaces) put
   `param_services` on every node of every image.
2. `57c19cd7df` (2026-11-16) then lowered a C/C++ image's `param_services`
   heap from 192 KiB to 112 KiB: "the 64 KiB default plus the 46,848-byte
   param_services peak". That peak was measured on `zephyr_cpp_params`, a
   ONE-node image.

The parameter services cost is per NODE. Each of a node's six services owns a
request/reply buffer pair of `PARAM_SERVICE_BUFFER_SIZE` (4,096 by default).
`cpp-derived` serves them on four nodes. A fixed per-image Kconfig default
cannot price a per-node cost, and the arm's own help says so ("nothing in
Kconfig knows what an image declares").

The parameter store is not the consumer: both images carve it, and its
services' state, from the executor backing (nros 175 confirms its carve branch
does not touch this road).

### The fix, as ruled (user ruling 2026-11-21, relayed by the manager)

- **Class fix (b).** The `param_services` heap floor scales with what the
  image DECLARES. The floor is a base plus `nodes_serving_param_services` x a
  per-node cost MEASURED from two boot-record peaks, one 1-node image and one
  4-node image. It is delivered as a derived rung of `NROS_ZEPHYR_HEAP_SIZE`
  from the model's node count, through the existing knob ladder, not a second
  carrier. The runtime peak above that floor stays operator-stated.
- **Stopgap (a), until (b) lands.** The two failing images state a measured
  heap: their boot-record peak + the 24,576-byte floor.
- The flat 112 KiB arm from `57c19cd7df` is the 1-node case of (b), and its
  text is to say so.

## Closes when

Item 3 is either shown to pass solo (and recorded as load-sensitive) or
fixed. Item 2 is closed (issue 2768).
