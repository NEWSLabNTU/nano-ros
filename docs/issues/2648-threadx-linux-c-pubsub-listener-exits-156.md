---
id: 2648
title: "The threadx-linux C pubsub listener sometimes exits 156 (-100, TRANSPORT_ERROR) 0.3 s after boot, and the one log of it predates the fix that names the cause"
status: open
type: bug
severity: low
area: [tests, threadx, rmw-zenoh]
related: [2420, phase-559]
found: 2026-11-16
---

**Home:** [phase-559](../roadmap/phase-559-ci-workflow-organization.md), filed
from W12's acceptance run at the manager's direction. The cell is red about
1 run in 8 by the manager's count.

## Measured

`merge-runtime.yml` run 43716062858 (`lane=tier1`, branch
`ci-w12-evict-probe-between-stages` at 1d968501f6, 2026-11-15, started
12:52Z), shard `threadx_linux`, junit artifact
`merge-runtime-tier1-threadx_linux-junit`:

```
zenohd: accepting on tcp/0.0.0.0:9100 103 ms after spawn
[threadx_linux c] pubsub: starting talker/listener...
thread 'test_rtos_pubsub_e2e::platform_3_Platform__ThreadxLinux::lang_2_Lang__C' panicked at packages/testing/nros-tests/tests/rtos_e2e.rs:1107:5:
threadx_linux E2E failed — readiness pattern 'Waiting for messages' not observed.
Started with:
  .../nros run --plan /tmp/nros-run-X44kNH/plan.json ... --router none  # image: .../examples/workspaces/c/build/threadx-linux-zenoh-pubsub_bringup/cmake/threadx_listener_entry
Output so far (truncated):
[app_define] Creating byte pool...
[app_define] Running board network init...
[app_define] Creating app thread...
[app_define] App thread created, returning to kernel...
[app_thread] Calling app_main (weak)...
[app_thread] app_main returned, exit code 156
nros run: the target exited with code 1 after 0.3 s
```

- 156 is `-100` as an 8-bit exit status, `NROS_CPP_RET_TRANSPORT_ERROR`, the
  catch-all for an RMW failure the app did not map.
- The router was listening 103 ms after spawn, before the listener started.
- The listener gave up 0.3 s after boot. That is far shorter than any session
  open timeout, so it was not waiting for a router that never came.

## Not known: the cause

The run predates #3984 (merged 17:17Z the same day, `ffc0f3ebcb`). Before
#3984, a ThreadX C or C++ image dropped the runtime's own log records, so the
listener printed no `zenoh open: …` line, and the log cannot say whether the
locator, the router or the handshake failed. #3984 fills the ThreadX log slot
before `app_main`, so the next red names its cause.

There has been no red since. The cell passed on every threadx_linux run after
#3984: `merge-runtime.yml` 43750588189, 43761024137 and 43773321880 (tier1
shards), and `nightly.yml` 43808375964 (`ThreadxLinux`). At about 1 in 8, four
greens in a row happen about 59 % of the time, so they do not show a fix.

Not this issue, but adjacent: issue 2420 found this same cell holding port
9100's lease while `rmw_coordinate_truth` waited for it. The lease worked
there, so a shared-port collision is a lead to check against the next log,
not a finding.

## Next

Read the first red after #3984 for its `zenoh open` line, then fix from what
it names. Until one arrives, a local loop of this one cell (its fixture
built, run solo and then under a parallel load) is the way to get a
reproduction sooner.
