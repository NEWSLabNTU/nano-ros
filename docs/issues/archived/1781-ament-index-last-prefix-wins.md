---
id: 1781
title: "`AmentIndex::from_path_string` lets the LAST `AMENT_PREFIX_PATH` entry win
  — ament's rule is the FIRST — so a colcon overlay's copy of an interface package
  is silently replaced by the underlay's (`/opt/ros`)"
status: resolved
type: bug
area: cli, codegen
severity: high
found: 2026-10-10
related: [1780, phase-485]
---
## Measured

phase-485 M2. A scratch prefix holding a modified `std_msgs` (complete ament
package, `resource_index` entries copied from the host) was put FIRST on
`AMENT_PREFIX_PATH`, the way a sourced colcon overlay is. `nros sync --verbose`
printed `codegen AMENT pkg std_msgs` and generated the `/opt/ros/humble`
version: the added field was absent. With the same prefix LAST, the field was
generated.

## Cause

`packages/cli/rosidl-bindgen/src/ament.rs`, `from_path_string`: it walks the
prefixes in path order and does `packages.insert(package.name.clone(),
package)` for each, so a later prefix OVERWRITES an earlier one. Ament (and
`ament_index_python`, colcon's `local_setup`) resolve a package from the FIRST
prefix that has it — that is what makes an overlay an overlay.

## Consequence

Any user who builds an interface package in their own workspace that also
exists in the underlay — a patched `std_msgs`, a newer `rcl_interfaces`, a
vendor fork of a message package — gets the UNDERLAY's generated code, with no
warning, while their ROS nodes use the overlay's. A wire mismatch with no
diagnostic.

## Fix

Keep the first prefix's package (`entry().or_insert`), and add a test with two
prefixes holding the same package. Check the other `AMENT_PREFIX_PATH` readers
for the same shape (`git grep -n AMENT_PREFIX_PATH -- packages/cli`:
`cargo-nano-ros/src/{package_discovery,provider_scan,workflow}.rs`,
`nros-cli-core/src/{cmd/build,cmd/ws,orchestration/prereq_resolve}.rs`).

## Resolved (2026-10-10)

The first prefix wins, in every reader that picks one:

- `rosidl-bindgen` `AmentIndex::from_path_string` — `entry().or_insert`.
- `cargo-nano-ros` `discover_installed_ament_packages` (the `rust_packages`
  index) — same, split into `discover_installed_ament_packages_in(path)` so the
  test needs no process environment.
- `cmake/NanoRosGenerateInterfaces.cmake` step 2 APPENDED every prefix's
  interface files, so a package in an overlay and its underlay got both
  copies; it now stops at the first prefix that has the package.

Already first-wins: `NanoRosCodegenCore.cmake` and
`find/_NrosFindRosMsgPackage.cmake` (return on first hit), and the cross-layer
`AmentIndex::merge`. Order-insensitive: `prereq_resolve::ros_packages` (a name
set) and `generate-rust-incremental.sh` (a signature over every prefix).

Guards, both FAILING with the old `insert` restored:
`first_prefix_wins_so_an_overlay_shadows_its_underlay` (rosidl-bindgen) and
`installed_ament_packages_first_prefix_wins` (cargo-nano-ros). End to end, the
phase-485 M2 overlay put FIRST now generates its added field via `nros sync`,
and the host install alone reverts it. The cmake change was checked with a
`cmake -P` probe of the loop (one `String.msg`, the overlay's), not with a cmake
build.
