# Bundled interface packages

Vendored ROS 2 interface **sources** (`package.xml` + `msg/*.msg`) so
`nros sync` / `cargo nano-ros` codegen works on hosts **without a ROS 2
installation** — the book's first-node flow ("no ROS 2 needed") depends on
this: without it, `std_msgs = "*"` falls through the patch table to
crates.io, which only carries a yanked, unrelated `std_msgs` crate.

- Source: ROS 2 Humble (`/opt/ros/humble/share/`), `std_msgs` 4.9.1 +
  `builtin_interfaces` 1.2.2. License: Apache-2.0 (see each `package.xml`).
- Only the codegen inputs are vendored (`package.xml`, `msg/`) — no cmake,
  no IDL, no prebuilt bindings.
- A sourced ROS 2 environment always **takes precedence**: the ament index
  is loaded first and these fill gaps only
  (`AmentIndex::merge` in `rosidl-bindgen/src/ament.rs`,
  `load_index_with_fallback` in `cargo-nano-ros/src/lib.rs`).
- History: the original bundled copy lived in the retired
  `packages/codegen` submodule and was silently lost when that submodule
  was removed — found by the issue #204 clean-system bootstrap probe.
