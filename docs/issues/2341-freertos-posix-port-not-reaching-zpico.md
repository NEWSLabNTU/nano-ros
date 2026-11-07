---
id: 2341
title: "The FreeRTOS POSIX board sets its port for CMake only, so a zenoh image stops in zpico-sys on `FREERTOS_PORT not set`"
status: open
type: bug
area: [boards, build, cmake]
severity: high
found: 2026-11-07
related: [issue-2287, issue-0941]
---

## Measured

A release asset built from `main` plus PR #3183 replays the FreeRTOS POSIX
starter: `nros new` a C talker for board `freertos-posix`, then `nros run`. The
build stops in nros-c's cargo step, inside zpico's build script:

```
thread 'main' panicked at .../packages/rmw/zenoh/nros-zpico-build/src/runner.rs:3102:13:
FREERTOS_PORT not set. FreeRTOS port, as upstream FreeRTOS-Kernel's enum (phase-349 W3). Examples: GCC_ARM_CM3 (Cortex-M3) | GCC_ARM_CM7 (Cortex-M7). A vendor port upstream does not list is its path under portable/
...
gmake[2]: *** [nano_ros/packages/api/nros-c/CMakeFiles/_cargo-build_nros_c.dir/build.make:70: ...] Error 101
nros build: `cmake --build build/freertos-zenoh-freertos-posix-fp_node_bringup/cmake` exited 2
```

`mps2-an385-freertos` passes on the same replay.

Reproduced in-tree, 2026-11-07: `nros new fp_ws --board freertos-posix
--template talker --lang c`, then `nros build`, stops the same way. The
configure says `board facts ... — 4 value(s) delivered to cargo`, and
`nros ws board-facts src/fp_node_bringup --board freertos-posix --deploy
freertos` prints `NROS_BOARD`, `NROS_BOARD_TOML`, `NROS_PLATFORM_NAME` and
`NROS_SDK_FREERTOS_KERNEL`. It prints no `FREERTOS_PORT` and no
`FREERTOS_CONFIG_DIR`.

## Cause

The FreeRTOS platform manifest
(`packages/platform/nros-platform-freertos/nros-platform.toml`) lists
`FREERTOS_PORT` and `FREERTOS_CONFIG_DIR` as `required_env`, and zpico's build
script panics when either is missing. Since phase-484 W2b these are board facts. Each
board states them as `[env]` rows in its descriptor's `cargo_config`, and
`nros ws board-facts` (`descriptor_env_rows` in
`packages/cli/nros-cli-core/src/cmd/board_facts.rs`) hands those rows to
Corrosion for the cargo half of a CMake build (`cmake/NanoRosBoardFacts.cmake`).
`mps2-an385-freertos`, `mps3-an536-freertos` and `s32z270-freertos` all state
them.

`packages/boards/nros-board-freertos-posix/nros-board.toml` has no
`cargo_config` and so no `[env]` rows. Its cmake module
(`cmake/board/nano-ros-board-freertos-posix.cmake:103`,
`nros_freertos_board_port(... "GCC_POSIX")`) sets the port for the CMake half
only. The cargo half never gets it.

### Not issue 2287

The board-facts answer does not depend on the build dir. The missing rows are
missing from the descriptor whatever the cmake coordinate is called.

### Latent since the move to the descriptor

The board's in-tree lanes are the two `cyclonedds` workspace fixtures
(`workspace-{c,cpp}-freertos-posix` in `examples/fixtures.toml`), and those
never build zpico-sys. Nothing built a zenoh image for this board, so nothing
asked for the port on the cargo road. The board's cmake module records the
road before W2b: the repo-wide `activate.sh` exported the Cortex-M port, which
is the wrong one for a host binary.
