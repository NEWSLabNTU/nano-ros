---
id: 109
title: "Config-driven bridge (`run_from_config`) opens every extra RMW session on domain 0 — `create_node_on` drops the configured `domain_id`"
status: resolved
type: bug
area: rmw
related: [phase-267, rfc-0009, 0107]
---

## Summary

`nros_bridge::run_from_config` built each bridge Node with
`Executor::create_node_on(name, rmw)`, which calls
`node_builder(name).rmw(rmw).build()` with `domain_id: None`. An **extra** RMW
session's participant domain follows the **node builder's** `domain_id`
(`resolve_session_slot` → `domain_id.unwrap_or(0)`), NOT the `SessionSpec`'s — so
a `[[node]]` declaring `domain_id = 5` still opened its participant on **domain
0**. The egress publisher then announced on the wrong domain and no domain-5
receiver matched, even though the topic + type were correct.

This is the same pitfall the imperative bridge bin works around with an explicit
`.domain_id(domain_id)` on the egress `node_builder` (see
`bins/bridge-zenoh-to-cyclonedds-fwd` + platform-implementation-notes "Domain
ID"). `run_from_config` was missing it.

## Repro (phase-267 ws-bridge-rust)

`demo_bringup/system.toml` puts the cyclonedds egress on domain 5
(`[[domain]] dds`). With the bug, `ros2 topic list` showed `/chatter` on domain
**0**, not 5; a `rmw_cyclonedds_cpp` subscriber on domain 5 received nothing.

## Fix (2026-06-28, phase-267 W-B3)

- Added `Executor::create_node_on_with_domain(name, rmw, Option<u32>)` — pins the
  extra session's participant domain via `node_builder.domain_id`. `create_node_on`
  now delegates with `None` (legacy domain-0 default preserved).
- `run_from_config` threads each `[[node]]`'s `domain_id` through all three node
  creations (the registration loop + per-bridge src/dst) via a `node_domain`
  lookup.

Verified: `/chatter` now announces on domain 5; the stock cyclone subscriber
receives the bridged samples end-to-end.
