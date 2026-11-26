---
id: 2976
title: "A zenoh publish on mps2/an385 costs 7 % more guest time at main than at bf9fccba83, most of it on the topic the publisher also subscribes to"
status: open
type: bug
severity: low
area: [zenoh, rmw, zephyr, qemu]
related: [issue-2971]
found: 2026-11-26
---

## What

Measured while resolving [issue 2971](archived/2971-qemu-violation-dispatch-cost-unexplained.md).
The violation-cpp `zephyr_mps2` image was built fresh at bf9fccba83 (2026-11-09)
and at d92c369a14 (main, 2026-11-26). Both ran 10 times solo under QEMU
`-icount shift=5,sleep=off`, which makes the guest clock count instructions, so
each run reads the same value to the microsecond. `on_timer` was instrumented with
`k_cycle_get_32()` around each call; that instrumentation is not committed.
Median of 120 ordinary ticks per side, in µs:

| call | bf9fccba83 | main | delta |
| --- | --- | --- | --- |
| `/t4/state` publish | 148 | 159 | +11 |
| `/t4/answer` publish | 148 | 160 | +12 |
| `/t4/avail` publish (the handler subscribes to it too, so local delivery) | 405 | 431 | +26 |
| console `printf` | 33 | 33 | 0 |
| whole dispatch | 735 | 786 | +51 (+7 %) |

The handler's code is the same at both ends for these calls: only the tick-6
busy-wait changed. Both builds used the same Zephyr workspace, which pins SDK 0.16.9 (the old
commit's build was pointed at it explicitly; main's resolved it itself, and its
compiler line was not recorded). So the growth is most likely in nano-ros's
publish path or zenoh-pico between the two commits; a bisect would confirm
it, and would also rule out the toolchain. The
largest share is on the publish that is also delivered locally. A remote-only
publish grew ~11 µs and the local-delivery one ~26 µs, so the local path grew
about twice as much as the network path.

## Why low priority

0.05 ms on a 100 ms period, and the contract on the path is 50 ms. It matters
only as a trend: every Cortex-M image pays it on every publish, and nothing
measures it, so the next growth would arrive unseen too.

## How to pick it up

A bisect over `bf9fccba83..d92c369a14` is 2,338 commits, about 11 steps, each a
fresh build of the one west leaf (`NROS_ZEPHYR_FIXTURE_FILTER=build-cortex-m-ws-cpp-violation-entry-zenoh just zephyr build-fixtures`)
plus one run. The fixed shift makes every step deterministic, so one run per
step is enough. Older commits need `just setup-launch-resolve` after the
checkout, and `ZEPHYR_SDK_INSTALL_DIR` pointed at the store's 0.16.9 SDK,
whose layout an older `nros` does not find. Estimate: half a day of one host.

The method and the run command are in issue 2971's Resolution.
A benchmark cell that publishes under a fixed shift and records the per-publish
guest time would turn this from an accident into a measured series. That cell
would be the durable fix, more than the bisect.
