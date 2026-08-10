---
id: 504
title: "Node code has no portable way to read a monotonic clock — control loops cannot compute dt without a per-platform extern hack"
status: open
type: enhancement
area: api
related: [issue-0502, issue-0503]
---

## Problem

Node packages are written against `nros` and are platform-agnostic by
design — the same pkg compiles into a POSIX, FreeRTOS, or Zephyr image.
But `nros` exposes no clock. The platform crates each have timing types
(`nros-platform-mps2-an385/src/timing.rs` `MonotonicClock`, etc.), and
the C ABI has `nros_platform_clock_us`, but neither is reachable from a
pkg without breaking platform-agnosticism: depending on a platform
crate pins the pkg to one target, and rclrs-style users coming from
`node.get_clock()` find nothing.

What portable node code needs it for, all encountered in a real 6-node
control workload (NEWSLabNTU/nano-ros-rt-eval):

- `dt` in control laws (velocity integration, PID) — currently forced
  to assume the nominal period, which silently misintegrates whenever a
  callback runs late;
- timestamping published state for end-to-end latency accounting;
- coarse in-node watchdogs ("last heard from X at t").

The workaround in that workspace is a dedicated crate declaring
`extern "C" { fn island_now_us() -> u64; }` with each entry binary
exporting an implementation — i.e., users re-inventing the platform
ABI that already exists one layer down.

## Fix direction

A minimal `nros::time` module over the existing universal export:

```rust
pub fn now() -> core::time::Duration  // from nros_platform_clock_us()
```

- `no_std`-clean, zero new platform work: every port already provides
  the symbol (the executor links it unconditionally).
- Monotonic-since-boot semantics, explicitly NOT ROS time — document
  that distinction; a future `Clock` abstraction with sim-time can
  build on top, but `dt`-grade monotonic time should not wait for it.
- Resolution inherits issue 0502 on FreeRTOS/ThreadX; fixing that
  makes this API honest on all ports.
