---
id: 113
title: "Config-driven bridge endpoints (`run_from_config` locator + domain) are baked, not env-overridable"
status: resolved
type: enhancement
area: rmw
related: [phase-267, rfc-0009, 0109]
resolved_in: phase-267
---

## Resolution (phase-267)

`run_from_config` now applies per-node env overrides over the baked config
(`config.rs::apply_node_env_overrides`, run right after parse): for a `[[node]]`
named `<N>`, `NROS_BRIDGE_<N>_LOCATOR` overrides its `locator` and
`NROS_BRIDGE_<N>_DOMAIN` its `domain_id` (`<N>` upper-cased, non-alphanumerics →
`_`; empty/unparseable values keep the baked value). A deployed bridge can now be
re-pointed at a different router / DDS domain without a rebuild.

The gated `declarative_bridge_zenoh_to_cyclonedds` test was updated to use an
ephemeral zenohd + `unique_ros_domain_id()` (overriding the baked `s0` locator +
`s1` domain), removing the fixed-port / fixed-domain collision caveat. Verified
end-to-end: forwarding on NON-baked endpoints (router :7600, cyclone domain 9) —
a stock `rmw_cyclonedds_cpp` subscriber on domain 9 received 7/7 `std_msgs/Int32`.
Unit tests: `env_key` + `apply_node_env_overrides`.

## Summary

`nros_bridge::run_from_config` reads each `[[node]]`'s `locator` and `domain_id`
verbatim from `nros-bridge.toml` (baked into the binary via the macro's
`include_str!`). There is NO runtime override. The imperative bridge bins, by
contrast, take `ZENOH_LOCATOR` / `ROS_DOMAIN_ID` from the env, so they can point
at a different router / DDS domain without a rebuild.

This bites both deployment and testing:

- **Deployment:** a declarative bridge entry is pinned to whatever router address
  and domains `system.toml` declared at build time. Re-pointing it at another
  router (different host/port) or domain requires editing `system.toml` + a full
  `nros sync` + rebuild.
- **Testing:** `tests/declarative_bridge_zenoh_to_cyclonedds.rs` cannot use
  `unique_ros_domain_id()` like the other cyclone host tests — it must pin zenohd
  to the baked port and the cyclone listener to the baked domain (`5`). Small but
  real concurrency caveat (a co-scheduled cyclone test could draw domain 5 and
  cross-talk).

## Fix direction

Let `run_from_config` honor per-endpoint env overrides, falling back to the baked
value. Options:

- **Env expansion in the config:** `nros sync` emits
  `locator = "${BRIDGE_S0_LOCATOR:-tcp/127.0.0.1:7447}"` /
  `domain_id = "${BRIDGE_S1_DOMAIN:-5}"` and `run_from_config_str` expands
  `${VAR:-default}` at load. Fully data-driven, self-documenting in the config.
- **Well-known env vars per session:** e.g. `NROS_BRIDGE_<NODE>_LOCATOR` /
  `NROS_BRIDGE_<NODE>_DOMAIN`, applied over the baked `NodeCfg` before
  `open_multi`. Simpler, but the override surface is implicit.

Either lets the test thread a unique domain + ephemeral router (removing the
domain-5 caveat) AND lets a deployed bridge be re-pointed without a rebuild.

## Discovered

phase-267 W-B test wave (the gated
`declarative_bridge_zenoh_to_cyclonedds.rs`). See that test's doc comment for the
baked-endpoint caveat.
