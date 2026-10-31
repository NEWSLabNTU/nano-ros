---
id: 2179
title: "The generic `zephyr` board's `nros-nsos` snippet reaches every mps2/an385 image of a leaf whose image says `board = \"zephyr\"`, and Kconfig refuses the build"
status: open
type: bug
area: [zephyr, boards, fixtures]
severity: medium
found: 2026-10-31
related: [2157, 2164]
---

## What

Issue 2157 put native_sim's NSOS in one snippet, `nros-nsos`, named by the
generic `zephyr` board (`packages/boards/zephyr/nros-board.toml`,
`[board.zephyr] snippets = ["nros-nsos"]`), and the module hook adds it to
`SNIPPET` for every rendered `system.toml` image
(`_nros_image_kconfig_merge_snippets`, `zephyr/cmake/nros_image_kconfig.cmake`).
The snippet is deliberately board-blind: on a non-native board Kconfig refuses
it, which 2157 measured on `mps2/an385` with a bare `west build -S`.

But the in-tree mps2/an385 fixture rows build leaves whose images say
`board = "zephyr"` -- e.g. `examples/zephyr/rust/talker` (`[image.zephyr_zenoh]
board = "zephyr"`) for the `talker-serial` row
(`west_build_name = "build-cortex-m-rust-talker-serial-zenoh"`, board
`mps2_an385`). So the hook adds `nros-nsos` to an mps2 build too.

Measured 2026-10-31, Zephyr 3.7, `west build --cmake-only -b mps2/an385` of
`examples/zephyr/rust/talker` with the row's `conf_files` on `origin/main`
(`1e4c8c7673` + later):

```
Merged configuration '.../cmake/zephyr/mps2-an385-serial.conf'
Merged configuration '.../zephyr/snippets/nros-nsos/nsos.conf'
...
nsos.conf:15: warning: attempt to assign the value 'y' to the undefined symbol NATIVE_SIM_SLOWDOWN_TO_REAL_TIME
error: Aborting due to Kconfig warnings
```

With the board's `snippets = []` (a local edit, not committed) the same
configure completes. Every mps2/an385 row over a leaf whose image names the
generic `zephyr` board is expected to fail the same way; only the serial row
was built.

## Direction

The board the image names is `zephyr` while the west BOARD is `mps2/an385`,
so either the snippet must follow the west board (a native-sim board package
names it, the generic one does not), or the mps2 rows must build an image
whose board is the mps2 package (`nros-board-mps2-an385-zephyr`). Whichever,
a fixture row that builds a non-native board must be in a lane that would
have caught this; check which lane builds `talker-serial` and why it stayed
green (or did not run) after 2157.

## Sighting on main: live-peer regression (2026-10-31)

live-peer regression run 41167433083 (schedule, `380cde09c6`), job
124438293461 `rows whose board is NOT this runner`, step `Build the fixtures
those rows resolve`, failed on the first Zephyr fixture,
`zephyr/mps2_an385/c/talker/zenoh` (`build-cortex-m-c-talker-zenoh`):

```
Merged configuration '/__w/nano-ros/nano-ros/zephyr/snippets/nros-nsos/nsos.conf'
nsos.conf:15: warning: attempt to assign the value 'y' to the undefined symbol NATIVE_SIM_SLOWDOWN_TO_REAL_TIME
error: Aborting due to Kconfig warnings
✗ Zephyr fixture build FAILED (exit 2)
```

So the defect also reaches the C leaves (`examples/zephyr/c/talker`, image
`zephyr_zenoh`, `board = "zephyr"`), not only the Rust serial row this issue
measured, and the board-elsewhere live-peer lane cannot build any fixture
until it is fixed.
