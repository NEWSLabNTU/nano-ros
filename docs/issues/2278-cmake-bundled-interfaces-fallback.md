---
id: 2278
title: "CMake and west guest apps still generate messages from the bundled interfaces when ROS is not sourced; configure refuses the same case"
status: open
type: enhancement
area: cli, cmake, zephyr, book
purpose: [education, industrial]
related: [2275, phase-530, phase-532]
found: 2026-11-05
---

**Home: [phase-530](../roadmap/phase-530-nros-configure-single-pass.md)**, which owns the configure refusal from issue 2275.

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
