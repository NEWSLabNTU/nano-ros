---
id: 9
title: Test groups are fully serialized due to shared resources
status: resolved
type: tech-debt
area: testing
related: [phase-74, phase-75, phase-140]
resolved_in: Phase 74
---

Fixed by Phase 74 (Test Infrastructure: Parallel Isolation). QEMU-based E2E
tests now run in parallel across platforms using Slirp networking (74.1 — no
TAP/bridge/sudo), per-platform zenohd ports (74.2 —
baremetal=7450…zephyr=7456), and per-platform nextest groups (74.5;
serial within a group, concurrent across groups). C/C++ library contention
was resolved by Phase 75 (relocatable CMake install), then superseded by
Phase 140 (per-example in-tree Corrosion staticlibs, no shared prefix; build
dir cleaned per invocation).

Remaining serialization is by design: `c_api`/`cpp_api` (shared static-lib
build outputs), `xrce` (single Agent UDP port), `large_msg` (CPU/memory
stress), `ros2-interop` (ROS 2 discovery contention).
