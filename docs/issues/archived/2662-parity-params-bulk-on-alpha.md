---
id: 2662
title: "The ros2_cli parity params cases fail since issue 2639's 70 bulk parameters landed on `/alpha`"
status: resolved
type: bug
area: [params, testing]
severity: medium
found: 2026-11-17
resolved: 2026-11-17
resolved_in: "the change that filed it"
related: [phase-478, issue-2639]
---

**Home: [phase-478](../../roadmap/phase-478-island-developer-experience-findings.md), I22.**
GitHub #4091. A regression from issue 2639's change (ea94566e87), found on
main 2026-11-17.

## What happened

Issue 2639 gave the `param-two-node-talker` fixture 70 `bulk.pNN`
parameters, on `/alpha`, so `params_per_node_interop` could prove stock
`ros2 param list` / `ros2 param dump` see every name past 64. Two
`ros2_cli_parity_interop` cases (cell
`native-ros2-cli-parity-params-rust-zenoh`) diff `/alpha`'s answers against
the stock rclpy peer (`fixtures/ros2-param-peer/param_peer.py`), which
declares only `/alpha`'s original parameters. Both failed on the 70 extra
lines:

- `declared_parameters_answer_ros2_param_as_stock_does`: "the declared
  parameter names: nano-ros and stock ROS 2 answer `ros2 param list {node}`
  differently", `-   bulk.p00` .. `-   bulk.p69`.
- `use_sim_time_compared_with_stock`: its held gap is "the listings differ
  by `+   use_sim_time` alone", and they now differed by the bulk lines too.

## Resolution (2026-11-17)

The bulk parameters live on a third fixture node, `bulk`, registered after
`alpha` and `beta`. `/alpha` again declares exactly what the rclpy peer
declares, so both parity cases mean what they meant before issue 2639.
`ros2_param_list_and_dump_answer_every_name_past_64` asks `/bulk`, and
`params_per_node_interop` waits for the image's three parameter-service
markers instead of two.

Moving the parameters was chosen over filtering `bulk.` lines out of the
parity diffs: a filter would leave the parity cases comparing a node stock
does not have, and every future parity case on `/alpha` would need it too.
