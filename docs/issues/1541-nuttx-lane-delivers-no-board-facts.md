---
id: 1541
title: "The NuttX cargo lane runs `cargo build` with no board facts — the Zephyr-arm shape `check-board-facts-delivery` was written for"
status: open
type: bug
area: [build, boards]
severity: medium
found: 2026-09-28
related: [phase-472, 0460]
---

## What happens

`packages/api/nros-c/cmake/nros-nuttx.cmake` builds the NuttX image through
`cmake -E env … cargo build`, and delivers none of the board facts
(`NROS_BOARD_TOML`, `NROS_PLATFORM_NAME`, …) that `nros ws board-facts` emits for
every other lane. The Corrosion imports in nros-c's, nros-cpp's and the zenoh
staticlib's `CMakeLists.txt` carry none either.

`check-board-facts-delivery` reads only `cmake/*.cmake` and `zephyr/cmake/*.cmake`,
so it cannot see these files. Its own docstring's lesson is that the Zephyr arm
once shipped inert for exactly this reason. Copying either file into `cmake/`
makes the gate fail on it; at its real path it passes.

## What is NOT established

**Whether NuttX images need the facts.** If the NuttX lane resolves every knob it
consumes from somewhere else, this is a gate gap only. If any `nros-node` /
`nros-params` knob reaches a NuttX image through the RFC-0049 ladder, then NuttX
images are taking builtin defaults — issue 1388's class — and this is a live bug.
The audit did not build a NuttX image to find out.

## Fix

Establish which of the two it is, then either deliver the facts or record why the
lane does not need them; widen the gate's population in either case (phase-472 W5).
