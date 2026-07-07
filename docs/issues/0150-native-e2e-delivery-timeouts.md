---
id: 150
title: "Native e2e delivery timeouts on the dev machine — XRCE cross-process, zenoh→cyclone bridge, safety-integrity, mixed-QoS lanes"
status: open
type: bug
area: testing
related: [issue-0053, issue-0135]
---

## Summary

After the 2026-07-08 env-resync (fixtures fresh, preconditions fixed), a
cluster of native e2e tests still fails with RUNTIME delivery timeouts —
processes start cleanly, publishers publish, subscribers receive nothing
within the window:

| family | tests | symptom |
| --- | --- | --- |
| `c_xrce_api` | 3 (`talker_listener_communication`, `service_request_response`, `action_fibonacci`) | talker + listener both start and print; listener never receives 3 messages (12 s timeout). Agent = `build/xrce-agent`. `test_c_xrce_talker_starts` / listener-start PASS — only cross-process delivery dies. |
| `bridge_zenoh_to_cyclonedds` | 3 (`_e2e`, `_ros2`, `_to_nano_listener`) | panics at the delivery assertion after ~11 s. |
| `cpp_c_safety_integrity_e2e` | 2 (C + C++ CRC-validated subscription counts) | delivery/count assertion. |
| `mixed_qos_workspace_e2e` | 1 (`mixed_qos_matched_delivers_cross_process`) | cross-process delivery timeout. |

## Why this is its own issue

These are NOT precondition/staleness failures (that class was burned down in
the resync — see `env_machine_test_debt` memory + commits e8ac4fb11,
532d54f61): binaries are freshly built, agents/routers start, and the
failures are deterministic delivery timeouts. Candidate causes to
discriminate, per lane:

- **XRCE**: `build/xrce-agent` is an old build — version/CDR drift vs the
  current micro-xrce client pins? Rebuild the agent first
  (`just xrce ...`/scripts), then re-run; if still dead, wireshark the
  agent port.
- **bridge/cyclone + safety + mixed**: these lanes cross RMWs or processes
  with cyclone on one side. Check domain-id plumbing first (the
  [[project_multirmw_bridge_extra_session_domain]] class: egress session
  defaulting to domain 0), then whether the runner env leaks
  `ROS_DOMAIN_ID`/`NROS_DOMAIN_ID` between the resync's parallel runs.

## Repro

```
cargo nextest run -p nros-tests --test c_xrce_api
cargo nextest run -p nros-tests --test bridge_zenoh_to_cyclonedds
cargo nextest run -p nros-tests --test cpp_c_safety_integrity_e2e --test mixed_qos_workspace_e2e
```
