---
id: 2017
title: "FreeRTOS level images keep configUSE_TIME_SLICING at the kernel default"
status: resolved
type: enhancement
area: [freertos, executor]
severity: low
found: 2026-10-24
resolved: 2026-10-25
related: [phase-509, rfc-0108, 2018]
---

## Measured

The phase-509 P10 FreeRTOS item asks for derived stacks and for
`configUSE_TIME_SLICING = 0` when the image has levels. The derived stacks
landed (`NROS_LEVEL_STACK_BYTES`, `tier_task_memory_freertos.h`).

Time slicing did not. No `FreeRTOSConfig.h` in the tree sets
`configUSE_TIME_SLICING`, so the kernel default of 1 applies. The setting is
kernel-wide and is compiled into the kernel library, not into the generated
entry. No build input currently tells the kernel build that an image has
levels.

Levels hold distinct priorities, so slicing only matters between a level and
a non-nros task at the same priority, such as the lwIP or zenoh-pico read
tasks. Every worker parks each pass, so a level still yields. With slicing
on, though, a level can be sliced against an equal-priority task in the
middle of a callback, and RFC-0108 D6 asks the kernel not to do that.

## Closes when

A level image's FreeRTOS build sets `configUSE_TIME_SLICING 0`, or the board
config states why the default is kept. That needs a per-image input the
kernel build can read. The FreeRTOS realtime rows must stay green.

## Resolution

A FreeRTOS level image's kernel is built with `configUSE_TIME_SLICING 0`;
every other FreeRTOS image keeps the kernel default, 1.

**The per-image input.** `NROS_FREERTOS_LEVEL_IMAGE`, read by every board's
`FreeRTOSConfig.h` (the shared family file, so mps2-an385, mps3-an536 and
s32z270, plus the POSIX simulator's own file). Delivered on both build roads:

- **cmake** (C/C++ entries): the entry's link-libs sidecar sets
  `NROS_LEVEL_IMAGE` on the executable when the entry is tiered, and
  `nano_ros_entry` defines `NROS_FREERTOS_LEVEL_IMAGE=1` on
  `freertos_config`, the carrier every kernel TU reads (issue 1197).
- **cargo** (Rust entries): `nros build` adds the board crate's
  `level-image` feature for a multi-tier schedule (`schedule_board_features`,
  the predicate `nros::main!`'s multi-tier arm uses), forwarded to
  `nros-board-freertos`, whose build.rs defines it on every `cc::Build`.

**Assertions.** `tier_task_memory_freertos.h` (included only by tiered
FreeRTOS entries) refuses to build against a slicing kernel unless the host
states `NROS_FREERTOS_ALLOW_TIME_SLICING`; `freertos_run_tiers.c` refuses a
config that ignores the input; `nros-board-common`'s
`a_level_image_kernel_does_not_time_slice_and_others_keep_the_default` runs
each board's `FreeRTOSConfig.h` through the host preprocessor both ways; the
CLI's `a_level_image_asks_a_freertos_board_for_the_level_kernel` holds the
feature to the four board crates. Both level runners print the kernel's
setting at boot (`nros: kernel time slicing OFF`).

**Equal-priority tasks.** lwIP's `tcpip_thread` blocks on its mailbox, the
netif poll task on a task notification (poll interval as a ceiling),
zenoh-pico's lease task sleeps and its read task blocks in `recv` with the
socket timeout. None spins without blocking; the read task runs back to back
only while frames are pending, bounded by the socket buffer
(`Z_RX_TASK_BUDGET_FRAMES` adds a rest where a board states it).
