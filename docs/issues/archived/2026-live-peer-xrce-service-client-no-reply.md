---
id: 2026
title: "Live-peer: the nano-ros XRCE service client gets no reply from an rclpy server (`test_ros2_service_xrce_client`)"
status: resolved
type: bug
area: [testing, rmw-xrce, interop]
severity: medium
found: 2026-10-24
resolved: 2026-10-24
related: [issue-0672, 2009, 0778, phase-466]
---

**Home: [phase-466](../../roadmap/phase-466-scheduled-lanes-produce-verdicts.md)** — its live-peer lane cannot go green while this cell is red.

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

## Measured (2026-10-24, solo, this host, ROS humble + a ROS-paired Agent)

`test_ros2_service_xrce_client` on main at `7ddf4f42c0`: **5 of 5 failed**,
each with nine `Service call failed, retrying: Runtime` while the rclpy server
reported ready. Not a flake.

## Cause

Issue 2009 (b827142a64) made every client match a reply to its request by
sequence id and DROP a reply that names no pending request. For XRCE that id
comes from `xrce_reply_callback`, which since issue 0778 recorded the wrong one
of the two ids Micro-XRCE's `on_reply` passes:

- `request_id` is the id of the `uxr_buffer_request_data` READ that delivered
  the reply (`read_access.c` passes the DATA submessage's own request id);
- `reply_id` is the id `uxr_buffer_request` returned when the request was
  SENT, decoded from the reply's `BaseObjectRequest`.

The callback stored the first and discarded the second. Nothing compared the
value until 2009, so 0778's mistake was invisible for two months; after 2009
every XRCE service client (Rust, C and C++ alike: they share the backend)
dropped every reply.

## Fix

`xrce_reply_callback` records `reply_id`. New buildless C test
`tests/reply_correlation.c` (`just check rmw-xrce`) calls the callback with the
two ids different and asserts `take_response` reports the sent one; with the
old line it reports `3` for a request sent as `42` and fails.

Measured after: `test_ros2_service_xrce_client` **5 of 5 pass** (3.4 s each).
The whole `xrce_ros2_interop` binary: 7 of 8 pass. The eighth,
`test_ros2_action_xrce_client`, receives the result and fails only its
`"Goal accepted"` grep, a line the example stopped printing with issue 0448;
it is not a recorded live-peer cell, and that stale assertion predates this
change.
