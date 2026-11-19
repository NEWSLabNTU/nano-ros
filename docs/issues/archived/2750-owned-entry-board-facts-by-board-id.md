---
id: 2750
title: "An owned entry's workspace configure asks board facts for the board id, so an image named otherwise is refused"
status: resolved
resolved: 2026-11-20
resolved_in: "the change that archives this issue"
type: bug
severity: medium
area: [cli, cmake, build]
related: [2433, 2751, 2752, 2754]
found: 2026-11-19
---

Part of the owned-entry run road theme: an image whose `[image.<id>] entry`
names a hand-written C/C++ package, built and run on the CMake road. The
other parts are 2751 (its census), 2752 (`nros run` finds no
executable) and 2754 (the root adds other bringups' node packages).

## Measured

nano-ros 93368ca929, a downstream island workspace (branch `nano-ros-next`,
CLI built from source). One bringup with two images on board
`freertos-posix`, each naming its own owned C++ entry package (a
hand-written `main()` over `nano_ros_add_executable(... ROS2_MAIN)`):

```toml
[image.unit_test-freertos-posix]
board = "freertos-posix"
entry = "asi_unit_test"
```

`nros run test_bringup:unit_test-freertos-posix` stopped in the workspace
configure, in the owned package's interface generation:

```
CMake Error at .../cmake/NanoRosBoardFacts.cmake:227 (message):
  nano-ros: board facts for .../src/test_bringup --board
  freertos-posix --deploy freertos-posix could not be resolved (`nros dev
  image board-facts`):

  Error:
  .../src/test_bringup/system.toml:
  no [image.freertos-posix] naming a board
Call Stack (most recent call first):
  .../cmake/NanoRosGenerateInterfaces.cmake:701 (nros_resolve_board_facts)
  .../cmake/NanoRosCodegenCore.cmake:1447 (nros_generate_interfaces)
  .../cmake/NanoRosVerbs.cmake:109 (nros_find_interfaces)
  .../cmake/NanoRosVerbs.cmake:187 (_nros_generate_declared_interfaces)
```

Renaming the image to its board id (`[image.freertos-posix]`) configures and
builds. That forces one bringup per program when two programs share a board.

## Cause

- The generated root states the owned entries' identity with
  `set(NROS_DEPLOY "<board>")`: `deploy` is the image's board, verbatim
  (`packages/cli/nros-cli-core/src/cmd/build.rs:2274`, issue 2433).
- `nros_resolve_board_facts` forwards `NROS_DEPLOY` as `--deploy`
  (`cmake/NanoRosBoardFacts.cmake:93`).
- `board-facts --deploy` matches the KEYS of the bringup's `[image.*]`
  table (`packages/cli/nros-cli-core/src/cmd/board_facts.rs:696`). The board
  id is a key only when the image happens to be named after it.

## Direction

Carry the image id where the image is meant: the root states it (as a
generated entry's configure has `NROS_IMAGE`), and board facts takes the
image, keeping `NROS_DEPLOY` the board for the entry verb. When one
configure holds several owned images on one board, any of them answers the
same board facts, so the first is enough.

## Resolution (2026-11-20)

The generated root also states `NROS_LEAF_IMAGE_ID "<image>"`, which
`nros_resolve_board_facts` already prefers over `NROS_DEPLOY`. So board
facts are asked for the IMAGE, and `NROS_DEPLOY` stays the board for the
entry verb.

`OwnedIdentity::agree` names the configure's first owned image. It now
REFUSES owned images of one configure that name different boards, naming
both images and both boards. That is asserted, never assumed, so no image
is built on another board's facts.

Reproduced on a scratch copy of `examples/linux/c/talker` with one image
`talker_main` on board `linux`. Before the fix: `no [image.linux] naming a
board`. After it, the configure and build pass.

Unit tests (`cmake_root`):
- the root states the image id before `nano_ros_workspace(`;
- agreement names the first image;
- two boards are refused.
