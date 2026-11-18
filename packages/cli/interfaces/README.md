# Bundled interface packages

Vendored ROS 2 interface **sources** (`package.xml` + `msg/*.msg`) so
`nros configure` / `cargo nano-ros` codegen works on hosts **without a ROS 2
installation** — the book's first-node flow ("no ROS 2 needed") depends on
this: without it, `std_msgs = "*"` falls through the patch table to
crates.io, which only carries a yanked, unrelated `std_msgs` crate.

- Source: ROS 2 Humble (`/opt/ros/humble/share/`), `std_msgs` 4.9.1 +
  `builtin_interfaces` 1.2.2. License: Apache-2.0 (see each `package.xml`).
- phase-327 W5 (issue 0368 F4) — the set must cover what the IN-TREE example
  workspaces depend on, else a ROS-less host cannot `nros configure` the repo's
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
  `rcl_interfaces` 1.2.3 (rcl_interfaces `82776fc9`, the commit
  `action_msgs` and `lifecycle_msgs` came from) joined for the stock node
  interfaces: the runtime makes the parameter services, `/parameter_events`
  and `/rosout` from its types, and a backend that needs a generated
  descriptor for every type (Cyclone) refuses an image it cannot find them
  for. Without it, a build opted in to these copies could not give a
  Cyclone image those interfaces. Byte-identical to upstream at that commit. One Humble copy
  serves every edition, as for the others: against Jazzy (`7aa3caf4`, the
  pin in `docs/reference/editions/jazzy/pins.toml`) the types the runtime
  uses differ only in `Log.msg`'s level constants (`byte` there, `uint8` in
  Jazzy, both one octet; constants are not in the REP-2011 type hash), and
  Jazzy's logger-level services, which the runtime does not serve, are absent.
  All Apache-2.0. If an example grows a NEW interface dep, vendor it here in
  the same change — the acceptance is `nros configure examples/workspaces/<ws>`
  green with no `AMENT_PREFIX_PATH`.
- Only the codegen inputs are vendored (`package.xml`, `msg/`) — no cmake,
  no IDL, no prebuilt bindings.
- **Opt-in since issue 2278.** With no ROS 2 sourced, configure refuses to
  generate from these copies in place of a ROS install (a bundled copy picks
  a distro silently, and a peer built from another one disagrees on layout
  and type hash, issue 2275). A build uses them only when
  `NROS_INTERFACE_SEARCH_PATH` names this directory. The test
  `runtime_interface_packages_resolve_from_the_opted_in_bundle`
  (nros-cli-core) checks both halves for the runtime's packages.
- A sourced ROS 2 environment always **takes precedence**: the ament index
  is loaded first and these fill gaps only
  (`AmentIndex::merge` in `rosidl-bindgen/src/ament.rs`,
  `load_index_with_fallback` in `cargo-nano-ros/src/lib.rs`).
- History: the original bundled copy lived in the retired
  `packages/codegen` submodule and was silently lost when that submodule
  was removed — found by the issue #204 clean-system bootstrap probe.
