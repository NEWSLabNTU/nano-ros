---
id: 2693
title: "The stack-floor check is OFF for images TI Arm Clang builds"
status: open
type: limitation
severity: medium
area: [build, toolchain, stack]
related: [phase-575, issue-2400, phase-557]
found: 2026-11-18
---

**Home: phase-575 W3.**

## What happens

The stack-floor check (`NROS_STACK_FLOOR=CHECK`, `cmake/NanoRosStackFloor.cmake`)
works out each task's worst-case stack from GCC's per-function frames
(`-fstack-usage`, `.su`) and its call graph (`-fcallgraph-info=su`, `.ci`).
TI Arm Clang 5.1.1.LTS rejects `-fcallgraph-info`, which failed every C build
on the cargo road. The TI toolchain file
(`cmake/toolchain/arm-freertos-tiarmclang-cr5f.cmake`) therefore sets
`NROS_STACK_FLOOR OFF`.

That turns the check off for exactly the images no other nano-ros lane builds:
a vendor's R5F images, whose stacks nobody else measures.

## What we have toward a TI version

- Frames: tiarmclang does accept `-fstack-usage`, and writes GCC-format
  `.su` lines (measured: `app.c:3:main	16	static`).
- Call graph: tiarmclang has no `-fcallgraph-info`. TI's own route is the
  object-file display utility, `tiarmofd -g` (its XML output feeds TI's
  `call_graph` scripts), or the linker map. Neither has been tried yet.

## What would fix it

The walker takes its call graph from TI's tools when the compiler is
`TIClang`, and keeps the `.su` frames; the toolchain file then stops forcing
the check OFF. Until then, an image built with TI Arm Clang has no stack
floor, and its boot-time high-water print is the only evidence of its stacks.

## Progress (2026-11-22)

**The flag is fixed.** The cargo road now passes `-fcallgraph-info=su` to GCC
only, the rule the cmake and Zephyr roads already had, through one helper
(`_nros_stack_floor_c_frame_flags`). Other compilers get `-fstack-usage`
alone, which tiarmclang accepts. A CLI test configures the helper as GNU,
TIClang and Clang. The `.ci` files were never the walk's call graph, which it
reads from relocations; they only sharpen a frame's name.

**The check is still off, for a different reason.** The toolchain file keeps
`NROS_STACK_FLOOR OFF`, and its comment now gives the real reason: the walk
has never run on a TI Arm Clang build. Two things are unproven:

- **The link map.** `link_map_inputs` reads a GNU ld or LLD map, and refuses a
  map that names no input. tiarmlnk's map is TI's own format.
- **The objects.** The walk reads them with `llvm-readobj` and
  `llvm-objdump`. They are ARM ELF, so this probably works, but it has not
  been seen to.

This host has no TI toolchain and no fixture row builds with it, so the next
step needs a host that has one. Configure an R5F image with
`NROS_STACK_FLOOR=CHECK` set by hand, then read what the walk says about the
map. If it refuses the map, the walker needs a TI map reader, or a link-map-free
mode that takes the linked objects from the build.
