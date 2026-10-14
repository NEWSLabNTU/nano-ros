---
id: 1897
title: "A `params` image links the six parameter services' code, dormant: nros-node has one feature for the store and the services"
status: resolved
resolved: 2026-10-15
type: tech-debt
area: [params, core, memory]
severity: low
found: 2026-10-14
related: [phase-461, 1529, 1856]
---

## What was true

Phase-461 W6 gave a bringup the `params` capability axis: the parameter store
without the six `rcl_interfaces` services. On every road the image now builds
the store, registers no service, claims no queryable for it, and (on a road
that reads the descriptor or the Zephyr Kconfig) reserves no service sets in
the executor's carved home. Measured on the features workspace's
`native_rust_params` image, `params` against `param_services`: the same
`/chatter` value from the seeded store, no `/param_talker/*_parameters`
services, an empty `ros2 param list`, and the carved service home down from
33,400 B to 10,392 B.

What it still carries is the service CODE. `nros-node` has one cargo feature,
`param-services`, for both halves: `ParamState`, the store's slots and the
six servers' dispatch all sit in `parameter_services.rs` and the
`#[cfg(feature = "param-services")]` blocks of `executor/spin.rs`, and the
servers depend on `nros-rcl-interfaces`. So `nros/param-store`,
`nros-cpp/param-store` (issue 1529) and the C crate's single feature all
forward `nros-node/param-services`, and the server code links unreachable:
nothing sets `ParamState::requested`, so no spin creates a queryable.

## Cost

Flash only. RAM is not affected: the queryables, their inbox rings and the
service sets are what the axis removes, and those are what this image no
longer has. Not measured yet; the size of the dormant code is the first
number this issue needs.

## What closing it takes

Split nros-node's feature in two, `param-store` (the store, `declare_*`, the
launch-seed and `--ros-args -p` paths, `use_sim_time` seeding) and
`param-services` (the six servers, their reconcile, `nros-rcl-interfaces`),
with `param-services` implying `param-store`; then point each forward above at
the new nros-node feature. `ParamState` holds both halves today, so the split
starts by moving the server table out of it.

## Fix (2026-10-15)

nros-node has two features now: `param-store` (the store, `declare_*`,
launch seeds, argv overrides, `use_sim_time` seeding) and `param-services`
(the six servers, their reconcile, `/parameter_events`, `nros-rcl-interfaces`),
which implies it. The store's executor state (`ParamState`, `ParamHome`,
`Placed`, `ArgvParamOverride`) MOVED out of `parameter_services.rs` into a new
`param_store.rs`, with the service fields of `ParamState` behind
`param-services`; nothing is duplicated. `nros`, `nros-c` and `nros-cpp` each
forward the same-named nros-node feature (nros-c's rclc parameter API is a
server and stays on `param-services`; its `param-store` exists so its size
probe agrees with nros, issue 0665). The Zephyr module, `nros_feature_set` and
the features workspace's Rust param node follow.

The old ways of saying "held, not served" are retired, not kept beside the
feature: `nros_params::PARAM_SERVICES_SERVED`, the `NROS_CAPABILITY_PARAM_SERVICES=0`
reading behind it (issue 1856), phase-461 W6's descriptor read of
`store = "params"`, the `s0`/`s1` suffix of the store shape, and the two
`report_parameter_services_not_built` refusals, which a build without the
services can no longer reach. `node-std-tests` and
`check-param-services-heap-free` drive the store-only build by feature. The
Kconfig symbol stays: it still says the bringup declares the axis, which
implies a store and sizes the Zephyr heap.

Measured on `examples/workspaces/features`, image `linux_rust_params` (x86_64,
the image's optimized dev profile), `params` against `param_services`, both
from this tree: `.text` 864,499 B against 905,347 B (-40,848), `.rodata`
126,020 B against 140,028 B (-14,008), and 0 `parameter_services` /
`rcl_interfaces` symbols against 28 (23,950 B). The carved home is 944 B
(the store's state alone) against 33,400 B served, and 10,392 B on the
pre-split knob path. Run against a router: the same seeded `/chatter` value
(120), no parameter services, an empty `ros2 param list`. In the same copy,
the mixed C node's probe is refused by `<rclc_parameter/rclc_parameter.h>`
naming `param_services`, as it should be.
