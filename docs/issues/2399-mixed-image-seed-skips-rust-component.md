---
id: 2399
title: "A mixed C/C++/Rust cmake image's configure seed leaves out its Rust component, so the promise check refuses the build"
status: open
type: bug
severity: high
area: [cli, cmake, build, census]
related: [phase-530, 1228, 2395]
found: 2026-11-10
---

**Home: [phase-530](../roadmap/phase-530-nros-configure-single-pass.md)**. W5 made the CMake roads' configure seeds binding (the promise checks), and the seed's registered population is what misses the Rust component.

## What

Tier-1 job of main run 42697365095, row `workspace-mixed-native`
(`examples/workspaces/mixed`, image `demo_bringup:linux`, coordinate
`posix-zenoh-linux-demo_bringup`), failed at configure:

```
CMake Error at cmake/NanoRosPromise.cmake:231 (message):
  nros: this image's entity inventory differs from the answer `nros configure`
  composed before this configure, because this configure declares something
  configure could not see: it registered [c_add_server_pkg::... ;cpp_listener_pkg::...]
  and this configure registers [... ;rust_heartbeat_pkg::heartbeat::rust_heartbeat_pkg::Heartbeat]
```

Reproduced locally with `nros build demo_bringup:linux` on main. The seed
(`build/posix-zenoh-linux-demo_bringup/resolved/demo_bringup__union/resolved.cmake`)
and the configure's `cmake/nros/entity_inventory.cmake` differ in one value:
`NROS_DERIVED_MAX_LIVELINESS` is 12 in the seed ("7 node tokens") and 13 in
the configure ("8 node tokens").

## Why

Stage 3.5 composes a cmake image's seed over the rows the workspace's metadata
probe recorded for the packages the root adds
(`entity_inventory::registered_from_probe`). The cmake metadata probe serves C
and C++ components only: a Rust component's facts come from the cargo
harness, so the probe never `add_subdirectory()`s `rust_heartbeat_pkg` and its
`nros-metadata.json` has no row for it. The image's own configure does add the
package, and its `nano_ros_node_register(LANGUAGE RUST ...)` writes a row. So
the configure registers one component more than the seed, and the promise check
refuses, as W5 designed it to.

## Fix

`registered_from_probe` takes the workspace root and adds the Rust
registrations of the packages in scope, parsed from their
`nano_ros_node_register` calls the way workspace discovery parses them
(`Package::cmake_component_metadata`, language `rust`), skipping any
`(package, component)` the probe already has. Both stage-3.5 callers (a single
image and a coordinate's union) pass the root. The seed then composes over the
same population the configure registers, by the same code, so it states 13 as
the configure does.

Test: `a_rust_registration_joins_the_probe_rows` (a C and a Rust package; the
probe holds only the C row; with the workspace the Rust registration joins
once, and an out-of-scope package stays out).

Measured: `nros build demo_bringup:linux` in `examples/workspaces/mixed`
failed at configure before the change and builds after it.

## Not decided here

Whether 13 is one liveliness token more than the image needs. The configure
counts the registered Rust row as its own node token beside the model's
`heartbeat` node; the seed now matches that, which is what the image built
with before W5 made the seed binding. If it is an over-count, it is the
composer's, on both roads at once.

Acceptance: tier 1 on main builds `workspace-mixed-native`.
