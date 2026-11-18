---
id: 2483
title: "A nano-ros node with parameter services does not declare `use_sim_time`, which every stock node does"
status: resolved
type: bug
severity: medium
area: [params, interop]
found: 2026-11-13
resolved: 2026-11-18
related: [phase-556]
---

## What

Measured on 2026-11-13 (phase-556 first measurement, Humble,
`rmw_zenoh_cpp`, `--no-daemon`), nano-ros fixture `param-two-node-talker`
node `/alpha` against a stock node declaring the same `rate` and `stepped`:

```
$ ros2 param list /alpha            # nano-ros
  rate
  stepped
$ ros2 param list /alpha_stock      # stock
  rate
  stepped
  use_sim_time

$ ros2 param get /alpha use_sim_time
Parameter not set.
$ ros2 param set /alpha use_sim_time true
Setting parameter failed: Parameter was not declared and undeclared parameters are not allowed
```

A stock rclcpp or rclpy node declares `use_sim_time` (bool, default false) on
every node, and a launch file or `ros2 param set` switching a whole graph to
simulated time relies on that. On a nano-ros node the set is refused, so
the node stays on its own clock while the rest of the graph moves to `/clock`.
`ros2 param dump` differs by the same line.

rclcpp also declares four `qos_overrides./parameter_events.publisher.*`
read-only parameters; those are not part of this issue.

## Question for the fix

nano-ros has ROS time. Whether `use_sim_time` should switch it at runtime,
or be declared read-only with its build-time value, is a design choice. A
read-only declaration at least makes `list`, `get` and `dump` agree with
stock, and turns the set refusal into an honest "read-only" answer.

## Resolution (2026-11-18, phase-556 W11)

Ruling (user, relayed by the manager): `use_sim_time` switches ROS time at
run time, and the capability is on wherever parameter services are on.

- A new capability axis, `sim_time`, lowers to the `sim-time` feature of
  `nros`, `nros-c` and `nros-cpp` on every road (cargo, CMake, Zephyr with
  `CONFIG_NROS_CAPABILITY_SIM_TIME`) and joins the stock set
  (`node_interfaces::STOCK`), so it follows the W13 default and its
  opt-outs. With it the executor's existing seed declares
  `use_sim_time = false` on every node that serves parameters, and setting it
  true runs the existing phase-425 W3b reconciliation, which subscribes
  `/clock` and installs each sample as the image's ROS time.
- The `/clock` subscription is a runtime family priced once per executor
  and reserved whenever the capability is built in (`CLOCK_SUBSCRIPTIONS`,
  held to its one creation site `create_clock_sub` by
  `check-infra-queryable-counts`): the subscriber pool, the callback slots
  and the liveliness pool in the entity inventory; the word `clock` in the
  descriptor's `infra_families`; the zenoh liveliness default; the XRCE
  subscriber ring; and a depth-1 subscription entry in `nros-node`'s arena.

Measured (idle, `use_sim_time` false, the reservation alone; `just
mem-report` with `--baseline`, each pair built with and without `sim_time`
and otherwise identical):

| image | flash (text) | RAM (symbols) | where the RAM goes |
|---|---|---|---|
| `examples/linux/rust/talker`, declared model, `param_services` + `rosout` | +10,636 B | +4,385 B | executor arena +4,344 (one depth-1 subscription entry), `g_sessions` +40 |
| `examples/workspaces/rust` `freertos` image (MPS2-AN385, Cortex-M3), six components | +5,240 B | +9,826 B | executor arena +4,304, zenoh `SMALL_PAYLOADS` +4,096 (one more subscriber slot's ring), static subscriber storage +1,024, `SUBSCRIBER_BUFFERS` +280, `g_sessions` +112 |
| `param-two-node-talker` (owned entry, undeclared arena) | +21,656 B | +117 B | the arena is the undeclared budget either way; the cost is the code |

Flipped (`use_sim_time` true): no further static RAM, because the
subscription takes the reserved slots; the zenoh subscriber declaration's
heap use at that point was not measured.

## Acceptance

A phase-556 test compares `ros2 param list` and `ros2 param get use_sim_time`
for a nano-ros node and a stock node and finds the same answer. Until fixed,
that test is held in the strict expected-failure form.

Met by `ros2_cli_parity_interop::use_sim_time_answers_ros2_param_as_stock_does`
(plain check): `param list`, `param get use_sim_time`, an accepted
`param set use_sim_time true`, the value read back, and `topic info /clock`
showing one subscription on each side.
