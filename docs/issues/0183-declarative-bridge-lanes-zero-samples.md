---
id: 183
title: "declarative ws-bridge lanes deliver 0 samples (zenoh→cyclonedds nano listener + nested-header, zenoh→xrce)"
status: open
type: bug
area: testing
related: [phase-287, issue-0164]
---

## Summary

Deterministic (serialized rerun, fresh fixtures 2026-07-12):

- `declarative_bridge_zenoh_to_cyclonedds::declarative_zenoh_to_cyclonedds_bridge_to_nano_listener`:
  `expected ≥ 2 bridged samples to reach the nano cyclone listener (zenoh →
  declarative ws-bridge-rust entry → cyclonedds), got 0. Full listener
  output:` (EMPTY — the listener printed nothing at all).
- `…nested_header_to_ros2` — same lane, same shape.
- `declarative_bridge_zenoh_to_xrce::declarative_zenoh_to_xrce_bridge_to_nros_listener`.

The imperative `bridge_zenoh_to_cyclonedds::test_zenoh_to_cyclonedds_bridge_ros2`
and `demo_nodes_cpp_interop` failures from the parallel sweep PASSED
serialized (storm flakes) — only the declarative ws-bridge entries stay red.

## Notes

Empty listener output = the bridged-side listener process produced no stdout
at all → likely the ws-bridge-rust entry (or the listener fixture) never came
up rather than a forwarding bug. The ws-bridge workspace fixtures went
through the same fresh-sweep rebuild; check their entry build + the
`nros plan` wiring before suspecting the bridge runtime. Untriaged beyond
this; needs its own session.
