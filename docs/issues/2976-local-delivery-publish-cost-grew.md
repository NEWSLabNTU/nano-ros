---
id: 2976
title: "A zenoh publish on mps2/an385 costs 7 % more guest time at main than at bf9fccba83, most of it on the topic the publisher also subscribes to"
status: open
type: bug
severity: low
area: [zenoh, rmw, zephyr, qemu]
related: [issue-2971, phase-590]
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

## Release ruling (2026-11-26)

User ruling, relayed by the release owner: this is **not** an nros12 blocker.

- nros12 ships with it, listed in the release notes as a known issue.
- The bisect below runs after the release, not before it.
- The issue stays open after the release until the bisect or the
  benchmark cell below answers it.

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

## Bisect plan, prepared (2026-11-27)

Ready to run on host 241 once nros12 is published, beside the gate chain at
nice 19 and two jobs, one step at a time.

**Step.** One commit per step:

1. Check the commit out in a dedicated worktree.
2. Re-apply issue 2971's instrumentation. It wraps the three publishes in
   `on_timer` with `k_cycle_get_32()`, anchored on lines that are identical
   at both ends of the range, and a step whose anchors are missing is
   skipped, never measured.
3. Build the one west leaf fresh
   (`NROS_ZEPHYR_FIXTURE_FILTER=build-cortex-m-ws-cpp-violation-entry-zenoh`).
4. Run it three times under `-icount shift=5,sleep=off`, with a router on
   the image's baked port, 10885.
5. Report the median of the ordinary ticks (2 to 15, minus 3 and 6) per
   publish, and the spread between the runs.

At a fixed shift the runs should agree to the microsecond, so the spread
is a determinism check rather than a noise budget.

**Order.** First both ends, to reproduce the 405 to 431 µs on `/t4/avail`.
Then each ranked suspect, its parent against itself, stopping at the first
step of 8 µs or more. Only if none accounts for it, `git bisect run` over the
whole range, with "bad" meaning `/t4/avail` at or above 418 µs, the midpoint.

**Ranked suspects**, among the range's 2,347 first-parent commits (85 touch
`packages/rmw/zenoh`, 14 move the zenoh-pico pin):

| # | commit | why |
| --- | --- | --- |
| 1 | `59f236fce6` (issue 2824) | a publisher's source stamp moved to the WALL clock, a wall-clock read on every publish; a locally delivered sample may pay a second read for its receive stamp, which fits the local path growing twice as much |
| 2 | `c10f95d259` (issue 2596) | zenoh-pico bump: a send holds a transport use, a refcount per send |
| 3 | `145bb2d8b5` (issue 2614) | zenoh-pico bump: no send waits for the link holding the tx mutex |
| 4 | `ca657d15d0` (issue 2366) | a transient-local subscription re-reads a publisher cache: local-delivery bookkeeping |
| 5 | `1a896a6722` (issue 2760) | the shim's durations moved to the monotonic clock: 2824's pair |
| 6 | `4d5052bf07` | the Zephyr wall clock resolves after registration, which changes what a wall-clock read costs |
| 7 | `a5e0439885` (issue 2575) | zenoh-pico bump: batching before fragments |

The scripts are kept out of the tree, since the instrumentation is scratch by
2971's rule. Their location and the driver's three modes (`ends`,
`suspects`, `bisect`) are with the session that runs it.
