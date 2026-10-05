---
id: 1702
title: "A Zephyr C/C++ image that declares `param_services` links and then halts at boot: the 285,696-byte parameter store is not sized into `CONFIG_NROS_ZEPHYR_HEAP_SIZE`"
status: resolved
resolved_in: 2026-10-06
type: bug
area: [zephyr, sizing]
severity: medium
found: 2026-10-05
related: [1681, 1677, 1324, 1424, 1706, 1707]
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

## Resolution (2026-10-06)

### Reproduced first

On origin/main, with `features`'s hand-raised `CONFIG_NROS_ZEPHYR_HEAP_SIZE=524288`
taken out of the shared `prj-zenoh.conf`, `[image.zephyr_cpp_params]` (fixture
`workspace-zephyr-cpp-params`) halts 0.1 s in, against `rmw_zenohd`:

```
nros: HEAP EXHAUSTED (TOO SMALL): request 285696 bytes, arena 66048 bytes, free 54608 bytes, ...
nros: PANIC platform heap exhausted
```

### Two fixes weighed, one rejected after measuring it

**Moving the store off the heap was rejected.** One option was to make the store a named `.bss` static (`MaybeUninit`, claimed by the first executor), the way phase-392 W6 moved the executor backing. That is a move, not a saving, and it is only sound for images that actually build a store. It is not sound in general, because the static is reserved by every image that LINKS the declare path, not only by those that run it. Measured: a plain `cortex-m-cpp-talker-zenoh` (mps2, 64 KiB heap, which never builds a store and would have halted if it had) gained `NROS_PARAMETER_STORE` at 280,832 B of `.bss`. On a small MCU that is a link failure for an image that has no parameters. The heap pays only when a parameter is declared, which is the right cost model.

**Shrinking the store was not enough on its own.** Without the store, the image's own peak is still 111,360 B (boot record read live, `read-boot-report.py --heap-headroom`). That is above the 64 KiB default before the 24,576-byte headroom floor is added. Lifecycle alone costs about 64.5 KiB: five services, each with a 2 x 4,096-byte buffer pair. So the capability's heap need is real beyond the store, and the arena has to follow the declaration.

### What landed: the heap DEFAULT derives from the declared capability

- `zephyr/Kconfig` gains `NROS_CAPABILITY_PARAM_SERVICES` and
  `NROS_CAPABILITY_LIFECYCLE`, and `NROS_ZEPHYR_HEAP_SIZE` gains
  `default 524288 if NROS_CAPABILITY_PARAM_SERVICES` and
  `default 196608 if NROS_CAPABILITY_LIFECYCLE` (below Cyclone's 1 MiB, above
  Rust's 128 KiB). A conf fragment that STATES the heap still wins: a Kconfig
  default is only a default. The numbers are measured and the arithmetic is in
  the Kconfig help.
- The generated west application (`builder::west_app`, both the Rust and the
  C/C++ renderer) sets `CONFIG_NROS_CAPABILITY_<AXIS>` to `y` or `n` as a
  `CACHE ... FORCE` entry BEFORE `find_package(Zephyr)`. Zephyr reads it as a
  command-line Kconfig assignment. Every axis is always written, so dropping one
  resets it. The values come from `cmd::build::declared_capabilities`, the same
  reader that sets `NANO_ROS_FEATURES`. The carrier is the generated file and
  not a `-D` on `nros build`'s west line, because the fixture lanes re-run a
  configured build with plain `ninja`, which never re-reads a `-D`. No
  `set(ENV{})` is involved.
- `features`'s `prj-zenoh.conf` no longer states the heap. That line was issue
  1677's hand-edit.

### Measured after

Measured on a native_sim/native/64 build. Nothing in any conf states the heap:

| image | `.config` heap | live peak | verdict |
| --- | --- | --- | --- |
| zephyr_cpp_params | 524288 (derived) | 397,072 | boots, headroom ok (127,728 spare) |
| zephyr_rust_params / lifecycle / qos | 524288 (derived) | — | boot |

`entry_e2e::entry_matrix` returned `4 ran, 14 skipped, 0 failed`. The four that ran are zephyr cpp params (live-read 250), rust params (120), rust lifecycle and rust qos. The 14 skips are other platforms' cells and fixtures that were never built here.

There is no C sibling. No Zephyr image declares `param_services` in C (`c_params.launch.xml` is used on native only).

### What this does NOT cover, filed as issue 1706

- **Other RTOS heaps.** FreeRTOS heap_4 and the ThreadX byte pool take the same allocation. No in-tree FreeRTOS, ThreadX or NuttX image declares `param_services`, so nothing was measured there and no default moved.
- **C++ images that build a store without declaring the axis.** A C++ image always carries the store (issue 1529). A launch `<param>` or an app-side `declare_parameter` allocates it even when the bringup does not declare `param_services`, and in that case this derivation does not fire.
- **Sizing.** The default is per capability, not per store size. A bringup whose contracts declare `params:` derives a much smaller store (phase-446 W4) and gets the same 512 KiB. That is an over-size, the safe direction, and stating the heap overrides it.

Found while measuring, and filed as issue 1707: in the Zephyr fixture lane, a
generated west application is rewritten DURING or AFTER the lane's incremental
`ninja` builds. A declaration change therefore reaches an already-configured
image one lane run late. This affects `NANO_ROS_FEATURES` and these capability
symbols alike. A fresh build is unaffected.
