---
id: 2121
title: "Three rtos_e2e session cells fail in every tier-2 nightly since the #2094 fix landed"
status: resolved
type: bug
area: [ci, testing, zenoh, threadx, freertos]
severity: medium
found: 2026-10-28
related: [2094, 1013, 1991, 2125, 2126]
resolved: 2026-10-30
---

## Measured

Both tier-2 nightlies on main at `e0f81b4ac` failed on the same three
`rtos_e2e` cells, on all retries:

- nightly 40636297544 (05:09 UTC), job 122643934268;
- nightly 40648207713 (07:08 UTC), job 122681868613, `tier 2 nightly (pairwise cover)`.

Their junit (`tier2-nightly-junit`) names them as follows.

1. **`test_rtos_quiet_link_keeps_its_session::platform_3_Platform__ThreadxLinux`**
   ```
   assertion `left == right` failed: threadx_linux quiet link: a listener with no traffic opened 3 sessions in 75s.
   Router: ZENOH_CONFIG_OVERRIDE='…listen/endpoints=["tcp/0.0.0.0:9000"]' rmw_zenohd (port 9000; absolute path elided)
   ```
2. **`test_rtos_quiet_link_keeps_its_session::platform_1_Platform__Freertos`**
   ```
   freertos E2E failed — readiness pattern 'Application setup complete' not observed.
   [ERROR] nros: node declaration failed — NodeError::Transport(ConnectionFailed)
   ```
3. **`test_rtos_pubsub_e2e::platform_3_Platform__ThreadxLinux::lang_1_Lang__Rust`** (rtos_e2e.rs:949)
   ```
   threadx_linux rust pubsub E2E: the router log records fewer than two `New transport opened` lines, yet both nodes demonstrably talked to it
   ```

The other two real failures in those runs belong to issue 2111 (the an536
Cyclone C++ entry) and issue 2113 (the Zephyr a53 SMP cell). Row 13 of issue
1991 (the QEMU WCET bench) also fails, as it always does.

## Reading

The quiet-link cell (1 and 2) and the router-log session counting it shares
with the pubsub cell (3) arrived in `6d888adccf`, the #2094 fix merged on
2026-10-27. These two nightlies are the first tier-2 runs since then. Issue
2094's resolution says the cell passed on FreeRTOS, NuttX and ThreadX-Linux
locally. NuttX does not fail here.

One hypothesis covers 1 and 3, and it is NOT verified: two cells talking to
routers on the same port at the same time. The quiet-link router listens on
`tcp/0.0.0.0:9000`. If a concurrent ThreadX-Linux pubsub cell's nodes reach
that router, the quiet-link router sees extra sessions (1), and the pubsub
cell's own router log misses its `New transport opened` lines (3). The
nextest groups in `.config/nextest.toml` serialise `rtos_e2e` per platform.
Whether the new cell joined that group was not checked.

Failure 2 does not fit that hypothesis. The FreeRTOS listener never declared
its node.

## What it is NOT

- **Not a fixture build failure.** The fixtures were found and booted.
- **Not issue 2094 regressing on NuttX.** NuttX quiet-link is not among the
  failures.
- **Not the `ZENOHD_LOG` operator override** that failure 3's own message
  suggests. That was NOT checked either, and is the first thing to rule out.

## What would close it

- Run each of the three cells solo on a tier-2 runner, to separate a
  concurrency defect from a per-platform one.
- If concurrency: give the quiet-link cell its own port, or put it in the
  same nextest group as the `rtos_e2e` pubsub cells.
- Acceptance: all three pass in a tier-2 nightly.

## Resolution (2026-10-30)

No code of its own: issue 2125 fixed the cause, and issue 2126 is failure 3
filed again. The hypothesis above was right in kind, and the shared port was
the baked one, not a quiet-link choice. The quiet-link cell runs the pub/sub
Rust LISTENER image, which bakes its router port (7800 on FreeRTOS, 9000 on
threadx-linux), so it started its router on the same port as that platform's
`test_rtos_pubsub_e2e` Rust cell. `ZenohRouter::start_on_with` killed any
listener on its port as an orphan and truncated the shared
`zenohd-<port>.log`. That gives all three failures: FreeRTOS lost its router
mid-boot (2, `port 7800 still in use after kill attempt`), and the threadx-linux
cells each read a log the other had re-created (1 and 3). Fixed in #2802
(`5afe413c3c`): a router start on a named port waits for the port's `flock`
holder before killing anything.

Acceptance is met. The three cells pass in each of the three tier-2 nightlies
on main since that fix (`tier2-nightly-junit`, read 2026-10-30):

| nightly run | quiet-link FreeRTOS | quiet-link threadx-linux | pubsub threadx-linux Rust |
| --- | --- | --- | --- |
| 40833557587 (10-29 07:08) | pass 169.8 s | pass 147.4 s | pass 72.6 s |
| 41006780767 (10-30 05:08) | pass 169.9 s | pass 148.6 s | pass 72.6 s |
| 41018247377 (10-30 07:08) | pass 169.8 s | pass 147.4 s | pass 72.6 s |

NuttX quiet-link passed in all three as well (81 s). In `run-matrix.yml`'s
1-wise tier 2 (runs 40828659447 and 41013529125) these cells fail in about
0.1 s on every try, before any image could boot. That is the out-of-lane
`lane_skip!` path, which bare nextest reports as a failure. The panic text is
not in the job log, so this rests on the timing and on the run's junit rewrite
reporting `Real failures: 1 / 1` against dozens of 0.1 s FAIL lines.
