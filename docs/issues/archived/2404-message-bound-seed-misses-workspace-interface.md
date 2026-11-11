---
id: 2404
title: "The message-bound seed resolves a node's interface closure without the project's search path, so it misses the workspace's own message package and the promise check refuses the build"
status: resolved
type: bug
severity: high
area: [cli, cmake, build, codegen]
related: [phase-530, 2399, 1252]
found: 2026-11-10
resolved: 2026-11-11
---

**Home: [phase-530](../../roadmap/phase-530-nros-configure-single-pass.md)**. W1 (Ruling R45) made `nros configure` pass the interface search path as an argument, and W5 made the CMake roads' seeds binding; the message-bound seed's closure resolver kept reading the environment.

## What

Tier-1 job of main tier-2 run 42721642028, row `examples/workspaces/features`
(coordinate `posix-zenoh-linux-demo_bringup`), failed at configure:

```
CMake Error at cmake/NanoRosPromise.cmake:217 (message):
  nros: this image's derived message-bound sizes differs from the answer
  `nros configure` composed before this configure, over the same inputs.
```

Reproduced locally with `nros build demo_bringup:linux_c_qos_talker`, also on
a branch carrying issue 2399's fix. The seed
(`resolved/demo_bringup__union/resolved-message-bounds.cmake`) priced "15 of
32 types" and derived no knob; the configure's
(`cmake/nros/message_bound_knobs.cmake`) priced "15 of 33 types" and derived
the subscribed payload classes (`custom_msgs/msg/Reading=28`,
`std_msgs/msg/Int32=12`). The seed's input record named only
`builtin_interfaces` and `std_msgs` as tables.

## Why

`message_bound_knobs::write_west_seed_over` builds the closure from each node
package's `package.xml` through `cargo_nano_ros::resolve_interface_package_names`.
That function finds workspace interface packages only through
`NROS_INTERFACE_SEARCH_PATH` in the environment. Since phase-530 W1,
`nros configure` derives that path per project and passes it as an argument
(`cmd::configure::interface_search_path_for`). The configure process has no
such variable, so the seed's closure never saw the workspace's `custom_msgs`,
which seven of the features packages depend on. The host configure's
`nros_find_interfaces()` does find it, so the two composers priced different
closures.

## Fix

- `cargo_nano_ros::resolve_interface_package_names_in(package_xml, search_path)`
  takes the search path as an argument. The old name keeps its
  environment-reading behaviour for its other callers.
- `write_west_seed` / `write_west_seed_over` take the project's interface
  search path. The three stage-3.5 callers pass
  `interface_search_path_for(root)`; the standalone-leaf seed passes the
  leaf's.

Measured: `nros build demo_bringup:linux_c_qos_talker` in
`examples/workspaces/features` failed at configure before the change and
builds after it; the seed's tables now include `custom_msgs`.

Test: `the_search_path_argument_finds_a_workspace_interface_package`
(cargo-nano-ros): a node depending on a sibling interface package resolves it
with the search path and does not without it.

Acceptance: tier 1 on main builds the `examples/workspaces/features` rows.

## Verified

Tier-2 workflow run 42928442297 on main at `5e01d7ffed`, its `tier 1 (cells)`
job (130290459182): every `examples/workspaces/features` row of tier 1's lane
(the `workspace-features-{c,cpp,mixed}-*` rows) is selected, and
`just build tier1` passed. Only the later cells step failed, on unrelated
cells.
