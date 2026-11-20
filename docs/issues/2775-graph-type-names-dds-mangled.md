---
id: 2775
title: "On zenoh, the graph queries report a topic's type in its DDS-mangled wire form (std_msgs::msg::dds_::String_) where ROS 2 reports std_msgs/msg/String"
status: open
type: bug
severity: medium
area: [rmw, zenoh, graph]
related: [issue-2674, phase-381, phase-572]
found: 2026-11-21
---

## What happens

On the zenoh backend, every graph query that reports a type returns it the
way the liveliness token carries it: `std_msgs::msg::dds_::String_`.
`rmw_zenoh_cpp`'s graph, and so `ros2 topic list -t`, report
`std_msgs/msg/String`. That covers `get_topic_names_and_types`,
`get_service_names_and_types`, the four `*_names_and_types_by_node` forms and
`get_{publishers,subscriptions}_info_by_topic`'s `topic_type`. Topic and
namespace names were already demangled; types were not.

Measured by phase-572 W3's parity probe: the C, C++ and Rust component
surfaces all reported `chatter_type=std_msgs::msg::dds_::String_` for a
`std_msgs/msg/String` talker. All three agree because they share one
backend.

## Why

The Cyclone backend demangles (`graph_query.cpp`'s `demangle_type`). The zenoh
shim's grouping (`group_names_and_types`) and its endpoint-info query copied
the token's type through as-is. The cffi adapter (`rust_adapter.rs`) states
that a Rust backend reports ROS names, and refuses `no_demangle` for that
reason, so the zenoh shim broke the contract the adapter relies on.

## Fix

`Ros2Liveliness::demangle_type_name`, the inverse of `keyexpr::DdsTypeName`,
beside `demangle_topic_name`: `<pkg>::<kind>::dds_::<Name>_` becomes
`<pkg>/<kind>/<Name>`. Anything else passes through unchanged, and a ROS form
that does not fit is reported in the wire form, never truncated. Applied at the
two places a type leaves the shim (the grouping's type pass, and the endpoint
info), so every query and every language surface gets it. The parsed
entity keeps the wire form, because the matched-event and service-readiness
checks compare it against the local DDS name.
