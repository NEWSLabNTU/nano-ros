---
id: 53
title: Mixed-RMW bridge has no stock-cyclonedds variant + no "cross-RMW gateway" book recipe (211.I)
status: open
type: tech-debt
area: testing
related: [phase-211, phase-128, phase-129]
---

## Gap

`nros-bridge` (Phase 128/129) forwards across RMWs in-process. The landed
fixture + e2e (`bridge-zenoh-to-xrce-fwd` / `test_zenoh_to_xrce_bridge_e2e`)
prove the zenoh↔XRCE round-trip. Two pieces of the original 211.I headline use
case remained:

1. **Stock-cyclonedds variant** — the original "Autoware listener" framing
   replaces XRCE with stock `rmw_cyclonedds_cpp`. Needs the bridge to grow a
   cyclonedds egress; the in-tree fixture is zenoh+XRCE today, and the
   cyclonedds backend is C++/CMake-side and links differently. Deferred until a
   cyclonedds-enabled bridge fixture lands.
2. ~~**Book documentation** — a "cross-RMW gateway" recipe under
   `book/src/user-guide/`.~~ **DONE** — `book/src/user-guide/cross-backend-bridges.md`
   (281 lines): mental model (`rclcpp::Node` ×2), Rust/C/C++ build knobs, `NROS_RMW`
   env, memory/WCET budget, coverage matrix, troubleshooting; documents the
   `.rmw("cyclonedds")` (`-DNANO_ROS_RMW=cyclonedds`) Zenoh→Cyclone gateway.

## Remaining scope (2026-06-15) — piece 1 only

Piece 2 shipped. The substantive remaining work is the **zenoh→cyclonedds bridge
fixture + e2e** — a second bridge binary (`examples/bridges/tt-zenoh-to-cyclonedds`,
mirroring `tt-zenoh-to-xrce`) with a cyclonedds egress session, plus a
`test_zenoh_to_cyclonedds_bridge_e2e` round-trip. Tracked + in progress under this
issue (2026-06-15).

## Why deferred (from 211.I)

The zenoh↔XRCE round-trip is the proven foundation; the cyclonedds egress is a
distinct C++/CMake bridge-fixture build, not a quick add. Split out of Phase 211
(substantially complete + archived).
