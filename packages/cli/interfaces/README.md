# Bundled interface packages

Vendored ROS 2 interface **sources** (`package.xml` + `msg/*.msg`) so
`nros sync` / `cargo nano-ros` codegen works on hosts **without a ROS 2
installation** — the book's first-node flow ("no ROS 2 needed") depends on
this: without it, `std_msgs = "*"` falls through the patch table to
crates.io, which only carries a yanked, unrelated `std_msgs` crate.

- Source: ROS 2 Humble (`/opt/ros/humble/share/`), `std_msgs` 4.9.1 +
  `builtin_interfaces` 1.2.2. License: Apache-2.0 (see each `package.xml`).
- phase-327 W5 (issue 0368 F4) — the set must cover what the IN-TREE example
  workspaces depend on, else a ROS-less host cannot `nros sync` the repo's
  own examples (and, before the W5 guard, the failed sync silently NARROWED
  the tracked patch table). Added from the ros2 GitHub `humble` branches:
  `example_interfaces` 0.9.3 (`f8deb566`), `action_msgs` 1.2.3
  (rcl_interfaces `82776fc9`), `unique_identifier_msgs` 2.2.1 (`27767cef`).
  Completed same day when the W5 narrowing guard fired on the remaining two
  during the first full fixture build: `lifecycle_msgs` 1.2.3 (rcl_interfaces
  `82776fc9`), `geometry_msgs` 4.9.2 (common_interfaces `0843449`).
  `sensor_msgs` 4.9.2 (common_interfaces `0843449`) joined when the cmake
  find-stub gained its bundle rung and the local_msg_pkg compile-check
  fixture (which deliberately consumes AMENT pkgs) first ran ROS-less.
  All Apache-2.0. If an example grows a NEW interface dep, vendor it here in
  the same change — the acceptance is `nros sync examples/workspaces/<ws>`
  green with no `AMENT_PREFIX_PATH`.
- Only the codegen inputs are vendored (`package.xml`, `msg/`) — no cmake,
  no IDL, no prebuilt bindings.
- A sourced ROS 2 environment always **takes precedence**: the ament index
  is loaded first and these fill gaps only
  (`AmentIndex::merge` in `rosidl-bindgen/src/ament.rs`,
  `load_index_with_fallback` in `cargo-nano-ros/src/lib.rs`).
- History: the original bundled copy lived in the retired
  `packages/codegen` submodule and was silently lost when that submodule
  was removed — found by the issue #204 clean-system bootstrap probe.
