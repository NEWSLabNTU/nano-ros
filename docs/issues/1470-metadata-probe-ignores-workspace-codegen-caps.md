---
id: 1470
title: "The metadata probe never adds the package that supplies a workspace's
  `nros-codegen.toml` field caps, so a message the real build bounds reads as
  unbounded in the probe and the node refuses to compile"
status: open
type: bug
area: codegen, cmake, metadata
severity: high
found: 2026-09-24
related: [1469]
---

## What happens

The Autoware Safety Island bounds `std_msgs/Header.frame_id`. The board
build honours it. The metadata probe does not, and the node's own source
refuses to compile:

```
.../metadata-probe-cmake/build/nros-ws-autoware_vehicle_msgs/nano_ros_cpp/autoware_vehicle_msgs/msg/autoware_vehicle_msgs_msg_velocity_report.hpp:98:93:
error: static assertion failed: NROS_UNBOUNDED__autoware_vehicle_msgs_msg_velocity_report__field_header_frame_id:
autoware_vehicle_msgs/VelocityReport states no serialized-size bound -- unbounded member: header.frame_id (string).
A bound must EXIST before a buffer can be sized from it.
```

The cap is declared, in `src/island_interfaces/nros-codegen.toml`:

```toml
[fields]
"std_msgs/Header.frame_id" = { cap = 64, mode = "inline" }
```

The same message type therefore generates two headers that disagree, in one
workspace, at the same moment:

| generated header | `NROS_UNBOUNDED` markers | bound |
| --- | --- | --- |
| `build-board/island_interfaces/.../velocity_report.hpp` | 0 | `SERIALIZED_SIZE_MAX = 549`, marked DERIVED |
| `build/nros-metadata/metadata-probe-cmake/.../velocity_report.hpp` | 2 | none |

## Why, and why this is a decision rather than a patch

The island supplies its caps through an explicit
`nros_find_interfaces(CODEGEN_CONFIG ...)` in
`src/island_interfaces/CMakeLists.txt`. The probe never adds that package,
and `nros_workspace_interfaces()` has no workspace-wide notion of a codegen
config to pick it up from. So there is nothing for the probe to read even in
principle: the caps are attached to a package the probe does not build.

That makes this different from 1469, which was a mechanical duplicate the
board build already solved and whose fix reused the existing mechanism. Here
the question is where a workspace's field caps LIVE. Three shapes, none
free:

1. A workspace-wide codegen config the probe and the board build both read.
   Cleanest for the reader, and the largest change, since caps stop being a
   property of whichever package happened to declare them.
2. The probe discovers and adds any package that declares a
   `CODEGEN_CONFIG`. Smaller, but it makes the probe's input set depend on
   package discovery order, and two packages with disagreeing caps become a
   silent race.
3. The probe accepts an explicit caps path from the workspace. Smallest and
   most honest, and it pushes the question onto every workspace author.

This issue does not pick one. It records that the probe and the real build
disagree about whether a type is bounded, which cannot be right under any of
them.

## Why it matters

Field caps are the normal way to bound a ROS message for an embedded
target: an unbounded string has no serialized-size bound, and a buffer
cannot be sized from a bound that does not exist. So this is not an island
quirk. **Any workspace that bounds its messages the usual way cannot be
probed**, and phase-463's census reconciles a contract against exactly the
metadata the probe produces. With 1469 fixed the probe gets further and then
stops here, which is where the island stands today.

## Reproduce

With 1469's fix applied, in a workspace whose messages are bounded through
`nros-codegen.toml`:

```
nros sync -v      # static assertion, NROS_UNBOUNDED__..., in the node's own TU
```

The island is the case at hand; any workspace using caps should do.

## Acceptance

- The probe and the board build agree about whether a given message type is
  bounded, for every type in the workspace.
- The island's four nodes probe, which together with 1469 is what makes a
  census of this workspace possible at all.
- Whichever of the three shapes above is chosen is written down with its
  reason, because the next person to add caps to a workspace will need to
  know where they belong.
