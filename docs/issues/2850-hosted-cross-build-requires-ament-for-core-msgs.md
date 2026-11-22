---
id: 2850
title: "A hosted cross `nros build` refuses with NROS-0001 when no ROS 2 is installed, even for messages the tree already carries"
status: open
type: limitation
severity: medium
area: [cli, interfaces, build]
related: [phase-589, issue-1248]
found: 2026-11-23
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
