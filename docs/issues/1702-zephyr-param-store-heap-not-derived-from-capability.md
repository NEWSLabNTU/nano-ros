---
id: 1702
title: "A Zephyr C/C++ image that declares `param_services` links and then halts at boot: the 285,696-byte parameter store is not sized into `CONFIG_NROS_ZEPHYR_HEAP_SIZE`"
status: open
type: bug
area: [zephyr, sizing]
severity: medium
found: 2026-10-05
related: [1681, 1677, 1324]
---

## What was measured

Issue 1681 made a Zephyr C++ image declaring `param_services` LINK. The first
image it was measured on was a temporary `[image.zephyr_svc]` of
`examples/workspaces/cpp`: native_sim, zenoh, `service_server.launch.xml`,
`[system] features = ["param_services"]`, and the bringup's own
`prj-zenoh.conf`. That image boots and dies 0.7 s in, as the parameter
services register:

```
nros: HEAP EXHAUSTED (TOO SMALL): request 285696 bytes, arena 66048 bytes, free 50256 bytes, largest free block 50256 bytes
nros: PANIC platform heap exhausted
>>> ZEPHYR FATAL ERROR 4: Kernel panic on CPU 0
```

The same image with one extra fragment, `CONFIG_NROS_ZEPHYR_HEAP_SIZE=524288`,
boots, serves the six parameter services, and answers `ros2 param list` /
`ros2 param get` against `rmw_zenohd`.

## Why

It is issue 1677's allocation: `ParameterStorage<32>`, one `Box` of ~8.5 KiB
per slot, on `nros_platform_alloc`'s rlsf arena (`CONFIG_NROS_ZEPHYR_HEAP_SIZE`,
Kconfig default 65536). 1677 fixed it for the one conf whose images then built
a store (`features`'s shared `prj-zenoh.conf`, 524288). It did not fix the
CLASS: no derivation connects "this bringup declares `param_services`" to the
heap. Every other bringup that turns the axis on for a Zephyr image inherits a
64 KiB arena and halts at boot. The failure is late and quiet. It is no build
error; the board panics once it is running.

The link failure (issue 1681) was hiding this. Before 1681, no Zephyr C/C++
image with the axis got far enough to allocate.

## Fix direction

Size the arena from the declaration, as the entity pools are sized
(RFC-0100's sizing descriptor already knows the image's capability axes). Two
options:

- Derive a floor `CONFIG_NROS_ZEPHYR_HEAP_SIZE >= store + headroom` whenever
  `param_services` is on.
- Refuse at configure time when a stated heap is below that floor, naming the
  knob.

Do not hand-edit more confs: that is how 1677 left this open. Also check the
same question for `lifecycle` and for the other RTOS heaps (FreeRTOS heap_4,
ThreadX byte pool), since the store goes through the platform allocator on
each.
