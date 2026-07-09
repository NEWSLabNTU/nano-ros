---
id: 175
title: "Zephyr (native_sim) Cyclone action/service: server receives but the result round-trip never completes"
status: open
type: bug
area: rmw
related: [issue-0164, issue-0157, phase-286]
---

## Problem

On Zephyr native_sim CycloneDDS, a **service/action server receives the
request/goal but the client never completes** — the result/completion round-trip
is lossy. Discovery + request delivery work; the reply path does not.

## Evidence (2026-07-09 family re-run)

- `test_zephyr_dds_{c,cpp,rs}_action_e2e` —
  `server_received_goal=true, client_completed=false`. The goal reaches the
  server; the client never sees the result.
- `test_zephyr_cpp_service_server_to_client_e2e` — `client OK=1` of 3 expected:
  the client got at least one reply (so the basic reply path CAN work) but the
  run is short of the expected count. (The server-side `"Request"` marker was
  itself stale — fixed in the #164 sweep to `SERVICE_INCOMING_REQUEST_MARKER` —
  so the diagnostic now reads true instead of "server requests=0".)

phase_118 covers only pub/sub, so these action/service lanes have had no
recent last-known-good on fresh images — the completion gap could be a genuine
regression or long-standing.

## Suspects / direction

- The Cyclone reply/result writer→reader path on native_sim (NSOS sockets):
  service response topic, action `get_result` / feedback / status. Pub/sub works
  on the same fixture, so it is specific to the request-response entities.
- Actions never complete at all (0), service partially (1/3) — check whether the
  action result service is even discovered, and whether the service reply is
  dropped under load or lost to a transient-vs-volatile QoS mismatch (the #157
  class was ROS-form type names + domain collisions; this is past that).
- Bake distinct domains per role-set if not already (the #161 domain work) and
  confirm the result reader is declared before the server replies.

## References

`packages/testing/nros-tests/tests/zephyr.rs`
(`test_zephyr_dds_*_action_e2e`, `test_zephyr_cpp_service_server_to_client_e2e`),
issue #164 (re-triage), issue #157 (the earlier zephyr-cyclone service fix this
builds on), `packages/dds/nros-rmw-cyclonedds/`.
