---
id: 2041
title: "No rmw_qos_profile_t type: code that names it, or reads its fields, does not compile"
status: open
type: enhancement
area: [api, cpp]
severity: low
found: 2026-10-25
related: [phase-488]
---

## What

Humble-era rclcpp code passes the rmw struct to `create_service`, as in
`create_service<S>(name, cb, rmw_qos_profile_services_default, group)`.
Phase-488 W3 added the five `rmw_qos_profile_*` constants on the std surface.
Each one is an `rclcpp::QoS` built from `rclcpp::detail::qos_table`, so it
binds to every verb's `QoS` parameter. Autoware's
`autoware_external_cmd_selector` @0.45.0 compiles unchanged with them.

There is still no `rmw_qos_profile_t` type. These forms fail to compile:

- `rmw_qos_profile_t profile = rmw_qos_profile_default;`
- `profile.depth = 1;`, or any other field read or write (`history`,
  `reliability`, `durability`, `deadline`, …);
- `rclcpp::QoS(rclcpp::QoSInitialization::from_rmw(profile), profile)`;
- `qos.get_rmw_qos_profile()`.

`rmw_qos_profile_system_default` is also absent, because every field of it is
a sentinel; `qos_table` leaves it out for the same reason.

## Done when

The std surface has an `rmw_qos_profile_t` whose field names and enum values
are upstream's. The five constants are values of that type and still convert
to `rclcpp::QoS`. `QoS` converts back through `get_rmw_qos_profile()`. A probe
in `just check cpp` compiles the four forms above. If the type is deliberately
not taken, the porting guide says so instead, together with the replacement
edit.
