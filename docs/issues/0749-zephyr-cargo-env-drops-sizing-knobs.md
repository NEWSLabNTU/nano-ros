---
id: 749
title: "Zephyr lane drops five of six executor sizing knobs — every image builds 1024-byte subscription buffers and a 32-slot param store, and oversize samples are dropped silently"
status: resolved
type: bug
area: zephyr
related: [issue-0316, issue-0460]
---

# 0749 — Zephyr cargo env drops the sizing-knob class

Found by ASI (the reference consumer) on the FVP closed-loop demo: the Zephyr
island received 4 of its 5 subscriptions and never the 13.4 KiB Autoware
trajectory, with zero diagnostics anywhere — wire-level capture showed the
island's cyclone ACKing every fragmented sample it then threw away.

## Defects

1. **The knob class never reached the Zephyr cargo builds.** Issue 0316's fix
   resolved exactly ONE of nros-node's six `build.rs` knobs
   (`NROS_EXECUTOR_MAX_CBS`) into the curated cargo environment; the other
   five (`NROS_SUBSCRIPTION_BUFFER_SIZE`, `NROS_EXECUTOR_MAX_SC`,
   `NROS_EXECUTOR_MAX_NODES`, `NROS_PARAM_SERVICE_BUFFER_SIZE`) plus
   nros-params' `NROS_MAX_PARAMETERS` had no Kconfig and no
   `_nros_resolve_knob` row. A consumer exporting
   `NROS_SUBSCRIPTION_BUFFER_SIZE=16384` in its build driver got 16384 on
   every native lane (shell env inherits) and silently 1024 on Zephyr (the
   curated env drops unlisted knobs). ASI measured it directly: freertos-posix
   baked `DEFAULT_RX_BUF_SIZE = 16384`, zephyr-fvp baked `1024` from the same
   build script — and `MAX_PARAMETERS = 32` against a controller that
   declares ~150 (declare fails `Full(-5)` from the 33rd parameter on).
   This is the textbook 0316/0460 class: "fix the CLASS, not the reported
   site" — the 0316 fix listed one knob and left five siblings.

2. **Oversize samples are dropped with no diagnostic.** `try_recv_raw`
   correctly returns `BUFFER_TOO_SMALL`, but the C++ arena dispatch path
   (typed `bind_subscription` trampoline over the raw subscription) swallows
   every non-OK take. At transport level cyclone completes and ACKs the
   reassembled sample, then discards it — so the subscription looks matched
   and healthy from every outside probe (`ros2 topic info -v`, tshark ACKNACK
   analysis) while the app waits forever. Left open as the follow-up half:
   a throttled fail-loud log (RFC-0052 fail-loud rule) at the drop site.

## Fix (defect 1, landed with this issue)

`zephyr/Kconfig` gains `NROS_SUBSCRIPTION_BUFFER_SIZE`,
`NROS_EXECUTOR_MAX_SC`, `NROS_EXECUTOR_MAX_NODES`,
`NROS_PARAM_SERVICE_BUFFER_SIZE`, `NROS_MAX_PARAMETERS` (defaults = the
crate defaults, so nothing changes for images that never set them);
`nros_resolve_knobs()` resolves the whole class, environment-wins, into the
curated cargo env. Verified on ASI: zephyr-fvp rebuild bakes
`DEFAULT_RX_BUF_SIZE = 16384` / `MAX_PARAMETERS = 256`, and the island
receives real 13.4 KiB trajectories (MPC runs; closed loop drives).

## How it stayed invisible

Every prior "closed loop verified" measurement on the Zephyr lane ran the
controller in its input-starved or emergency-stop path: degenerate stopped
trajectories (2-11 points, under 1 KiB) FIT the 1024-byte buffer, so boot
markers, param seeding, control-rate measurements and `ros2 topic hz` all
looked healthy. The first drive attempt with a real on-lane route was the
first time a trajectory exceeded the buffer.
