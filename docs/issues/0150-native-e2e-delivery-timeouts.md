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

## Progress (2026-07-08 debug pass)

**XRCE lane RESOLVED.** Root cause: phase-266 W5b unified the compiled
default node name to `"node"` (replacing the unique `nros_{pid}` fallback),
and the XRCE backend derives its session key as `djb2(node_name)` — both C
demo processes hashed to the same key (`djb2("node") = 0x7C9B46AB`, observed
verbatim in the agent log), so the second client's CREATE_CLIENT rebound the
agent session and orphaned the first: the listener's topic callback never
fired (confirmed by instrumentation — no DATA submessage ever reached the
client). NOT a version skew: reproduced identically against agent v2.4.3
(pinned dist) and a source-built v3.0.1. Fix: `hash_session_key` now salts
the djb2 with `getpid()` on POSIX (embedded keeps the plain hash — one
client per device). `c_xrce_api` 5/5 green against the pinned agent.

**Safety-integrity lane RESOLVED.** Resolver/manifest drift: the four
`build_native_workspace_{c,cpp}_safety_{talker,listener}_entry` resolvers
looked in the default `build-workspace-fixtures` dir while the fixtures.toml
rows build into `build-workspace-fixtures-safety-{talker,listener}`. Fixed
to use `build_workspace_cmake_entry_in`. 2/2 green.

**Remaining (real, fixtures fresh):**
- both bridges' ZENOH INGRESS receives 0 samples — `bridge_zenoh_to_
  cyclonedds` panics "bridge forwarded 0 sample(s)" (the bridge's own
  counter) and `bridge_mixed_rmw` "xrce listener received 0 bridged
  sample(s)". Common factor is the bridge binary's zenoh ingress
  subscription; next probes: run the bridge manually with RUST_LOG against
  a live talker, check ingress QoS/RxO vs the talker, and the #53 egress
  domain lesson.
- `mixed_qos_workspace_e2e` 60 s timeout — unsampled beyond the timeout.
- `bins/bridge-zenoh-to-xrce-fwd` is not in `examples/fixtures.toml` — no
  recipe builds it (built manually this pass); add a manifest row.

## Repro

```
cargo nextest run -p nros-tests --test c_xrce_api
cargo nextest run -p nros-tests --test bridge_zenoh_to_cyclonedds
cargo nextest run -p nros-tests --test cpp_c_safety_integrity_e2e --test mixed_qos_workspace_e2e
```
