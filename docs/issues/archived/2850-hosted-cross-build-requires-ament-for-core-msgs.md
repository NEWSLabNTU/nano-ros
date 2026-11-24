---
id: 2850
title: "A hosted cross `nros build` refuses with NROS-0001 when no ROS 2 is installed, even for messages the tree already carries"
status: resolved
type: limitation
severity: medium
area: [cli, interfaces, build]
related: [phase-589, issue-1248]
found: 2026-11-23
resolved: 2026-11-24
---
Found by a downstream proof of concept on an embedded Linux camera SoC (HiSilicon Hi3519DV500: 2x Cortex-A55, Linux 5.10, musl 1.2.3, BusyBox), built as an out-of-tree hosted board; see phase-589.

## What
`nros build` for the camera board stopped at NROS-0001
(`Diag::AmentPrefixUnset`, `cmd/configure.rs`): `AMENT_PREFIX_PATH` unset and
no earlier configure recorded one. It is raised when the project has messages
to generate (`ws::has_messages_to_generate`), and that lookup asks a workspace
msg package, a generated crate under the build root, or the ament
`interface_index`. It never asks the core interfaces the tree commits under
`packages/interfaces/generated/` (`std_msgs`, `builtin_interfaces`, ...).

So a talker that uses only `std_msgs` needs a ROS 2 install on the build host,
which for a cross build to an embedded Linux target is the one place it has
no other reason to be. Issue 1248's rule is that the build asks only whether
ament packages are discoverable so message packages can be found; the core
packages are found without ament.

## Fix direction
A dependency the committed core interfaces satisfy is not a message to
generate, so it does not need ament.

## Acceptance
The Linux talker cross-builds for a musl target on a host with no ROS 2 and no
`AMENT_PREFIX_PATH`. A package that does need ament still gets NROS-0001.

## Resolution
The predicate (`ws::needs_ament_for_messages`, was
`has_messages_to_generate`) skips a dependency the bundled core interfaces
supply (`cargo_nano_ros::bundled_interface_packages`: std_msgs,
builtin_interfaces, geometry_msgs, sensor_msgs and the rest). That set is
closed (every dependency of a bundled package is bundled), and sync already
generates those crates from it on a host with no ROS 2. A workspace
interface package, or a dependency found only through the search path or an
earlier generated crate, still needs ament and still gets NROS-0001.

Measured: a fresh copy of `examples/linux/rust/talker` (`<depend>std_msgs`)
built with `nros build` with `AMENT_PREFIX_PATH`, `ROS_DISTRO` and every
`/opt/ros` PATH entry removed. Unit test
`a_project_on_core_messages_alone_configures_without_ament` fails with the
fix removed; the refusal test now uses a workspace interface package.

The musl half of the acceptance (the same build for an
`aarch64-unknown-linux-musl` board) waits on phase-589 W5's in-tree musl
board; the refusal does not depend on the target.

Known gap, not new: on a host with no ament, a dependency on a non-bundled
ROS package (e.g. `nav_msgs`) resolves to nothing, so it is not recognised
as a message package and gets no NROS-0001; sync then fails later for the
unknown package.
