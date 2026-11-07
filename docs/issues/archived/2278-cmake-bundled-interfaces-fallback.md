---
id: 2278
title: "CMake and west guest apps still generate messages from the bundled interfaces when ROS is not sourced; configure refuses the same case"
status: resolved
type: enhancement
area: cli, cmake, zephyr, book
purpose: [education, industrial]
related: [2275, phase-530, phase-532]
found: 2026-11-05
resolved: 2026-11-07
resolved_in: "fix-issue-2278 branch: the CMake road refuses the bundled interfaces unless NROS_INTERFACE_SEARCH_PATH names them"
---

**Home: [phase-530](../../roadmap/phase-530-nros-configure-single-pass.md)**, which owns the configure refusal from issue 2275.

## Measured

Issue 2275's decision (maintainer, 2026-11-05) keeps configure's refusal: when
`AMENT_PREFIX_PATH` is unset, no configure has recorded one, and the project
has messages to generate, `nros configure` (and so `nros build` and `nros run`)
stops (`packages/cli/nros-cli-core/src/cmd/configure.rs`, `recorded_env_to_restore_with`).
The reason given: generating from the host's ROS install keeps message layouts
and type hashes compatible with the ROS peer, and a bundled fallback would
silently pick a distro.

The CMake road does not follow that rule. With no ROS sourced, it still falls
back to the release's bundled interfaces:

- `cmake/find/Find<pkg>.cmake` (e.g. `Findstd_srvs.cmake`) searches
  `NROS_INTERFACE_SEARCH_PATH`, then `AMENT_PREFIX_PATH`, then the bundled set.
- `cmake/NanoRosGenerateInterfaces.cmake` resolves dependencies through
  "AMENT index (with bundled fallback)" (step 3,
  `${_NANO_ROS_PREFIX}/share/nano-ros/interfaces/<pkg>/`).

A Zephyr app built with plain west, or a CMake `add_subdirectory` consumer
(RFC-0121 D6, outside the workspace road), never runs configure. Its
`find_package(std_msgs)` therefore succeeds against the bundled copy, the very
case configure now refuses. A C node and the same node built through
`nros build` can end up with different message definitions on one machine.

The book (#3132) recommends sourcing ROS for a west app so the types match,
but cannot call it required, because the build does not require it.

## Next

Decide, then make the book state the rule:

1. **The CMake road refuses too.** `Find<pkg>` and
   `nano_ros_generate_interfaces` fail with the same message when neither
   `NROS_INTERFACE_SEARCH_PATH` nor `AMENT_PREFIX_PATH` names the package.
   The bundled set stays only behind an explicit opt-in, for example
   `NROS_INTERFACE_SEARCH_PATH` pointing at it.
2. **The bundled fallback stays for guest apps,** and the reason is recorded:
   who builds without ROS, and why a distro mismatch is acceptable there.

Either way, `ws::has_messages_to_generate` counts `std_msgs` as a message
"to generate" because the bundled set resolves it, so the bundled copies
already matter only as a gap-filler under ament. If option 1 is chosen,
check whether they can leave the release.

## Resolution

User ruling, 2026-11-07: option 1, the CMake road refuses too.

- **The refusal.** `find_package(<pkg>)` (the resolver in
  `cmake/find/_NrosFindRosMsgPackage.cmake`) and `nros_generate_interfaces`
  (`cmake/NanoRosGenerateInterfaces.cmake` and the Zephyr copy in
  `zephyr/cmake/nros_generate_interfaces.cmake`) stop when only the bundled
  copies hold a package that neither `NROS_INTERFACE_SEARCH_PATH` nor
  `AMENT_PREFIX_PATH` names. The check is
  `_nros_refuse_bundled_interfaces`, in the new
  `cmake/NanoRosBundledInterfaces.cmake`. It runs on the file list every
  tier produces: an explicit file, `nros_find_interfaces`'s `resolve-deps`
  closure, and auto-discovery. The remedy is configure's, word for word
  (`cmd/configure.rs`, `recorded_env_to_restore_with`): "Source ROS 2 in
  this shell (`source /opt/ros/<distro>/setup.bash`, or your overlay's
  `install/setup.bash`) and configure again". The message also names the
  opt-in. A package no layer holds at all still finds nothing silently, as
  before, for an optional dependency.
- **The opt-in.** The bundled set is reached only when
  `NROS_INTERFACE_SEARCH_PATH` (the cmake variable or the environment) names
  a bundled root, `packages/cli/interfaces` or `share/nano-ros/interfaces`
  under the nano-ros prefix. A named bundled root keeps its old place after
  `AMENT_PREFIX_PATH` and is not read as a first-layer root, so opting in
  changes nothing else. This applies to the find resolver,
  `nros_workspace_interfaces` and the workspace stub emission. On the Rust
  side, `load_index_with_search_path` drops the bundled directory from the
  first layer (`search_roots_without_bundled`) and merges it last, as
  before, so `resolve-deps` agrees.
- **Where this differs from configure.** Configure refuses only when
  `AMENT_PREFIX_PATH` is unset and none was recorded. Under a sourced
  ROS 2 install it still fills a package the install lacks from the bundled
  copies. Per the ruling, the CMake road refuses that gap-filler case as well.
- **`ws::has_messages_to_generate`.** It asks `interface_index`, which is
  search path, then ament, then bundled. So with no ROS, `std_msgs` counts as
  a message to generate only because the bundled set resolves it. That is
  what triggers configure's refusal, and the classification is correct. It
  is the reason the copies cannot simply be dropped.
- **Can the bundled copies leave the release? Not now.** They are still read
  in four places. `has_messages_to_generate` above uses them as a name
  catalog; without them a ROS-less project that depends on `std_msgs` would
  read as having nothing to generate and fail later, with a worse error.
  `nros build`'s `<depend>` preflight uses `bundled_interface_packages()`
  (issue 1304). The Rust road fills gaps under ament from them. And they are
  this opt-in. Removing them needs a decision on the gap-filler and a name
  list to replace the catalog. They were not removed.
- **In-tree builds opted in.** Only the fast-lane `probe-shared-types` gate
  configured CMake with the bundled set and no ROS
  (`tests/cmake-probe-shared-types-tests.sh`). It and its compile-lane sibling
  `tests/cmake-probe-workspace-caps-tests.sh` now name
  `$PROJECT_ROOT/packages/cli/interfaces` on their search path. CI images
  set `AMENT_PREFIX_PATH=/opt/ros/humble` (`ci/docker/ci-base`,
  `ci/docker/zephyr-ros`), so the lanes that build fixtures there resolve
  from ROS as before. The templates under `examples/templates/` are what a
  user copies, so they were left to follow the rule.
- **Book.** `guide/platforms/zephyr-west-app.md` now says sourcing ROS 2 is
  required, not just recommended, and names the opt-in.
  `tutorial/your-own-message.md` (the layer table and the shadowing
  contract) and `reference/message-generation.md` ("Where the definitions
  come from", plus a troubleshooting entry for the new message) state the
  rule.

Tests: `tests/cmake-bundled-interfaces-tests.sh`, the new fast-lane gate
`cmake-bundled-interfaces`, covers 8 cases: the refusal from `find_package`
and from `nros_generate_interfaces` by auto-discovery and by explicit file,
the opt-in through the environment and through the cmake variable, AMENT
answering with and without the opt-in, and the helper passing an opted-in
file. Cases 1, 5, 6, 7 and 8 failed before the change.
`cargo-nano-ros` `bundled_opt_in_tests::the_bundled_root_is_not_a_first_layer_root`
covers the Rust search path.
