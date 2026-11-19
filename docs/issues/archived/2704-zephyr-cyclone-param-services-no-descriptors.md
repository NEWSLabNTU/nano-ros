---
id: 2704
title: "A Zephyr image on `rmw = \"cyclonedds\"` with `param_services` boots without its parameter services: the Zephyr road generates no `rcl_interfaces` descriptors"
status: resolved
type: bug
area: [zephyr, rmw, params, cmake]
severity: high
found: 2026-11-18
resolved: 2026-11-19
resolved_in: "fix(issue 2704): the Zephyr road gives Cyclone the runtime's own interface types (#4223, 6069684b77); follow-up #4278 (767265fe42)"
related: [2304, 1268, 1271, 2705, phase-556]
---

## What happened

A downstream C++ image on Zephyr (FVP Base_RevC AEMv8-R, user-mode network),
`rmw = "cyclonedds"`, `[system] features = ["param_services"]`, one node
`/controller`, nano-ros at 9afb5c8a19, booted and then printed once:

```
<err> nros: parameter services could not be created for node /controller at
service get_parameters: Transport(Unsupported). `ros2 param` cannot reach 1 of
this image's 1 node(s). Not retrying: this failure cannot change without a
rebuild.
```

The image ran without any of its six parameter services, and the build was
green.

## Cause

The Cyclone backend creates an entity only for a type whose descriptor some
linked translation unit registered. The runtime's own types reached the
descriptor registry on two roads only: the cargo road (`nros-node` registers
each type at create time) and the top-level cmake road
(`nros_runtime_interface_types()`, issue 2304). `zephyr/CMakeLists.txt`
resolved the same `NROS_FEATURES` and turned on `param-services` in
nros-cpp/nros-c, but never called the helper, so no `rcl_interfaces`
descriptors were generated. Not a pin regression: the Zephyr road never had
it. `rosout` had the same hole on the cmake road (it publishes
`rcl_interfaces/msg/Log`, but the helper generated `rcl_interfaces` only for
`param_services`).

## Fix

- **#4223** (6069684b77):
  - The Zephyr module calls `nros_runtime_interface_types()` directly, not
    DEFERRED. A deferred call ran after the Cyclone descriptor-table cap was
    sized, so the table overflowed.
  - The helper also generates `rcl_interfaces` for `rosout`.
  - One table drives both roads,
    `packages/core/nros-orchestration-ir/data/runtime-interface-types.json`.
    It gives the axis, its packages and the types the executor registers.
  - Refusals:
    - `nros build` refuses an enabled axis whose runtime package is not on
      the interface index.
    - The cmake helper's FATAL_ERROR is the configure-time refusal.
    - The cap refuses, at configure, a registration that no longer fits.
    - The cargo road refuses a registration the C++ table cannot hold
      (`BuildError::DescriptorTableFull`) instead of dropping it.
  - Gate `check-runtime-interface-types-roads` holds every per-message
    generator's road to a call of the helper.
- **#4278** (767265fe42): one descriptor-table row per type. A cargo-road
  registration had taken two rows, while the table is sized at one row per
  counted type.

## Verified

The downstream FVP image, rebuilt at nano-ros main 93368ca929
(2026-11-19):
- no `parameter services could not be created` line and no descriptor
  overflow;
- the parameter store is carved;
- the image reaches "Actuation Safety Island is Live";
- the controller logs continuously through 121 s of uptime.
