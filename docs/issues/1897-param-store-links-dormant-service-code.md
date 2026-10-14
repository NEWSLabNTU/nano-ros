---
id: 1897
title: "A `params` image links the six parameter services' code, dormant: nros-node has one feature for the store and the services"
status: open
type: tech-debt
area: [params, core, memory]
severity: low
found: 2026-10-14
related: [phase-461, 1529, 1856]
---

## What is true

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
