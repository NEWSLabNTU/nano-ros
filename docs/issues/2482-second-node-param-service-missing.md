---
id: 2482
title: "In a two-node zenoh image the second node serves 5 of the 6 parameter services and publishes no `/parameter_events`"
status: open
type: bug
severity: high
area: [params, rmw-zenoh, interop]
found: 2026-11-13
related: [phase-556]
---

## What

Measured on 2026-11-13 (phase-556 first measurement, Humble,
`rmw_zenoh_cpp`, `ros2 ... --no-daemon`) against the workspace fixture
`param-two-node-talker`
(`packages/testing/nros-tests/bins/param-two-node-talker`), an image with two
nodes, `/alpha` and `/beta`, both with parameter services on.

`ros2 node info /alpha` lists all six parameter services. `/beta` lists five:

```
$ ros2 node info --no-daemon /beta
/beta
  Subscribers:

  Publishers:
    /beta_chatter: std_msgs/msg/Int32
  Service Servers:
    /beta/describe_parameters: rcl_interfaces/srv/DescribeParameters
    /beta/get_parameters: rcl_interfaces/srv/GetParameters
    /beta/list_parameters: rcl_interfaces/srv/ListParameters
    /beta/set_parameters: rcl_interfaces/srv/SetParameters
    /beta/set_parameters_atomically: rcl_interfaces/srv/SetParametersAtomically
```

`/beta/get_parameter_types` is missing, and
`ros2 service call /beta/get_parameter_types ...` times out. Reproduced in 3
runs. A stock node always serves all six, so `ros2 param list --param-type`
against `/beta` cannot work.

Also, `ros2 topic info -v /parameter_events` shows `/alpha` as the only
publisher: `/beta` publishes none, where every stock node publishes its own.
Whether one shared publisher per image is intended is not established; if it
is, it still differs from what a stock graph shows.

## Why nothing caught it

`params_per_node_interop.rs` drives both nodes with `param list`, `get` and
`set` only, which use the five services that ARE served. No test lists a
node's services against stock (phase-556 W0).

## Cause (not investigated)

Twelve parameter queryables for two nodes, and exactly the last one missing,
suggests a pool sized one short: the zenoh queryable count is derived from
the image's inventory (`ZENOH_MAX_QUERYABLES`, see the knob registry). Not
verified.

## Acceptance

A phase-556 test asks `ros2 node info` for each node of a two-node image and
finds all six parameter services, as stock does. Until fixed, that test is
held in the strict expected-failure form.
