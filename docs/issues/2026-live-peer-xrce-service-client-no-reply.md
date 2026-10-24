---
id: 2026
title: "Live-peer: the nano-ros XRCE service client gets no reply from an rclpy server (`test_ros2_service_xrce_client`)"
status: open
type: bug
area: [testing, rmw-xrce, interop]
severity: medium
found: 2026-10-24
related: [issue-0672]
---

**Home: none.** One sighting so far, in the scheduled live-peer lane.

## What failed

`live-peer regression` run **40061591574** (schedule, head `8cb3cd8fde`), job
**120776078051** (`rows whose board IS this runner`, the host job). Of the
seven jobs, this one failed and six passed. The host cells RAN, and the lane
reports `VERDICT: cells ran and FAILED`. One cell is red:

```
test test_ros2_service_xrce_client ... FAILED
panicked at packages/testing/nros-tests/tests/xrce_ros2_interop.rs:750:5:
nano-ros XRCE service client got no reply from the ROS 2 service server — XRCE-DDS reverse service interop regression.
  [INFO] nros: session open
  [INFO] Service call failed, retrying: Runtime
  [INFO] Service call failed, retrying: Runtime
```

The client is the Rust `examples/linux/rust/service-client` built for XRCE.
Every other host test in the job passed.

## What it is NOT

- **Not the previous red in this lane.** The 2026-10-19 to 2026-10-23 runs
  failed in the BOARD jobs, every one with `NO VERDICT: stopped in the build`.
  Their host jobs were green, so this is the first time a host cell has gone red.
- **Not issue 0672** (archived). That was a readiness wait that always passed
  instantly. The readiness wait is still in the test, and the client did open
  its session; it is the replies that never arrive.
- **Not proven to be a regression.** There is one sighting and no solo re-run.
  It could be a flake on the shared runner.

## Candidates (unmeasured)

Between the last green host job (2026-10-23, `6dde801c0f`) and `8cb3cd8fde`,
these commits touch the service and executor paths:

- the phase-509 P9b executor lock seam and band park (530df7182e, a4985e885f,
  db98368a0a, 70d1d098c2)
- b827142a64 #2009 (reply correlation by sequence id; C and C++ clients, so a
  Rust client is less likely to be affected)
- b1c3732d5b #2014 (action frame offsets)

## What would close it

Run `test_ros2_service_xrce_client` solo on the host runner at current main. If
it is red, bisect the window above. If it is green, record it as a flake, and
close only when the next scheduled live-peer run's host job is green.
