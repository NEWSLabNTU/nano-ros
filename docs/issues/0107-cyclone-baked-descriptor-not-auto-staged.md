---
id: 107
title: "Cyclone baked-default topic descriptor (`std_msgs/Int32`, …) does not auto-stage in a consumer binary → schema-free data-driven bridge (`run_from_config`) fails `PublisherCreationFailed`"
status: open
type: bug
area: rmw
related: [phase-267, rfc-0009]
---

## Summary

`nros-rmw-cyclonedds-sys/build.rs` bakes universal-fallback topic descriptors
(`std_msgs/Int32`, `rmw_dds_common_graph`) and links them
`+whole-archive,-bundle` so their `__attribute__((constructor))` register TU
"isn't dropped" (build.rs comment). In practice, in a CONSUMER binary the baked
descriptor's constructor does **not** run / stage at runtime: `find_descriptor`
(`descriptors.cpp`) returns null for `std_msgs::msg::dds_::Int32_`, so
`publisher_create` (`publisher.cpp:123`) fails with `PublisherCreationFailed`.

Every working Cyclone consumer **stages the descriptor explicitly** (e.g.
`bins/bridge-zenoh-to-cyclonedds-fwd` calls
`nros_rmw::register_type_descriptor("std_msgs/msg/Int32\0", INT32_FIELDS)` before
the raw publisher), so the broken baked-default fallback was never noticed.

## Why it blocks phase-267 C6

The declarative bridge runs `nros_bridge::run_from_config_str`, which is
**schema-free** — it has only the type NAME from `nros-bridge.toml`, not the
`&[Field]` schema `register_type_descriptor` needs. It cannot stage a descriptor,
so it must rely on the baked-default fallback — which doesn't stage. Result:
`BuildEntity("pub on s1: Transport(PublisherCreationFailed)")`.

Confirmed: adding explicit `register_type_descriptor("std_msgs/msg/Int32\0",
INT32_FIELDS)` (mirroring the imperative bin) to the bridge Entry makes the
Cyclone publisher create and the bridge run.

## Fix direction

1. **Make the baked-default ctor actually stage in consumers** — diagnose why the
   `+whole-archive` `__attribute__((constructor))` register TU does not run when
   `nros-rmw-cyclonedds-sys` is consumed via an rlib (link-flag propagation /
   force-link through the final binary). If fixed, `std_msgs/Int32` (the demo
   type) + `rmw_dds_common_graph` work in the data-driven path with no schema.
   Custom types still need (2).
2. **Wire `nros codegen cyclonedds-descriptors` into the bridge flow** — it
   already turns `.msg` → IDL → `idlc` → a `register.{c,h}` + manifest. `nros sync`
   would run it for each cyclone-side forwarded type when generating
   `nros-bridge.toml`, and the Entry build links the generated descriptor TU.
   General (any type), but a bigger integration.

(1) unblocks the C6 Int32 demo; (2) is the general data-driven-bridge descriptor
path. Related: the type NAME must be DDS-mangled (`std_msgs::msg::dds_::Int32_`)
in `nros-bridge.toml` — fixed in `render_bridge_runtime_config` via
`interface_type_name` (phase-267).
