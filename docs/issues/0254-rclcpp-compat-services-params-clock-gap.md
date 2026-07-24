---
id: 254
title: "rclcpp_compat lacks services, parameters, clock, and param-callbacks — real Autoware nodes can't compile via the compat shape"
status: open
type: enhancement
area: cpp-api
related: [issue-0253]
---

## Finding (autoware-safety-island-example ports, 2026-07-24)

Porting four real Autoware 1.5.0 nodes (github.com/NEWSLabNTU/
simple-autoware-safety-island) showed `rclcpp_compat.hpp` covers only
pub/sub/timer/QoS/log. Every ported node needed the `nros::ComponentNode`
shape instead because upstream uses, on `rclcpp::Node`:

- `create_service` / service clients (`async_send_request` + futures)
- `declare_parameter<T>` (no-default = required-param form) + `this->now()`
  / `get_clock()` / `rclcpp::Time` arithmetic / `rclcpp::Rate::period()`
- `add_on_set_parameters_callback` + `rcl_interfaces::msg::SetParametersResult`
  (runtime reconfigure — dropped in the ports)
- `rclcpp::create_timer(node, clock, period, cb)` free function

`ComponentNode` covers most of this (params facade, typed timers/subs), so
the gap is compat-surface aliasing + a clock story, not new runtime. Detail:
simple-autoware-safety-island `docs/porting-notes.md` entries 01/03/05/06.
