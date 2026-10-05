---
id: 1687
title: "A workspace publisher advertises KEEP_LAST(1) / TRANSIENT_LOCAL whatever its
  code or its launch plan declares — C, C++ and mixed QoS cells and the
  qos_overrides cell all red"
status: open
type: bug
area: [rmw, codegen, testing]
severity: high
found: 2026-10-05
related: [1651, 1684]
---

## Failing tests

- `nros-tests::workspace_features_e2e workspace_features::case_08_c_qos`
- `nros-tests::workspace_features_e2e workspace_features::case_13_cpp_qos`
- `nros-tests::workspace_features_e2e workspace_features::case_17_mixed_qos`
- `nros-tests::qos_override_e2e a_ros2_peer_sees_the_overridden_publisher_profile`

## Evidence

The three `workspace_features` cells declare `KEEP_LAST(10)` in code; the
`qos_override` cell declares `qos_overrides./qos_chatter.publisher.reliability
= best_effort` in the plan. `ros2 topic info -v` reports the SAME profile for
every one of them:

    Reliability: RELIABLE
    History (Depth): KEEP_LAST (1)
    Durability: TRANSIENT_LOCAL

(the matching `qos_listener` advertises `KEEP_LAST (4)`, so subscriptions are
not uniformly flattened). Neither the code-declared profile nor the plan
override reaches the advertised entity.

- CI: workflow_dispatch run 37252649866, tree of commit
  dc5691a584cfb9f2a53b4b92372370d81ee8cd38.
- Local, SOLO (`-j1`), fixtures built from that tree 2026-10-05 02:43–03:24
  UTC by `just build-test-fixtures lane=tier1`: all four red again, so not load
  and not a stale fixture.

## Where to look

`KEEP_LAST(1)` + `TRANSIENT_LOCAL` on a publisher is what a declared-QoS
contract row (phase-454 W12, `system.contract.yaml`) or a sizing-derived
default would impose, so suspect a layer that now overrides the per-entity
profile after the node sets it. Not bisected — no lane ran these between
2026-06-17 and now (issue 1651).

## Acceptance

All four pass solo and in the tier-1 `test-all`.
