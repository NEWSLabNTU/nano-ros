---
id: 1908
title: "NuttX images fail to link `nros_ported_main`: phase-499 W5's per-SOURCE `main` rename never reaches the cargo cross-build"
status: open
type: bug
area: [build, cmake, embedded]
severity: high
found: 2026-10-14
related: [1115, 0460]
---

## Measured

Run-matrix **38629298969** (workflow_dispatch on main `c68e520a3`), job
**116076495896** `tier 2 (1-wise matrix)`, step `just build tier2`. The nuttx
family's first error:

```
error: linking with `arm-none-eabi-gcc` failed: exit status: 1
  c_talker_app_main.c:(.text.nros_app_main+0x0): undefined reference to `nros_ported_main'
error: could not compile `nros-nuttx-ffi` (bin "nros-nuttx-ffi") due to 1 previous error
```

## Mechanism (read from the source, not yet built)

phase-499 W5 (`_nros_apply_app_main`, `cmake/NanoRosVerbs.cmake`) renames a
program's `main` to `nros_ported_main`. It does this as a per-**source**
property:

```cmake
set_property(SOURCE "${_src}" APPEND PROPERTY COMPILE_DEFINITIONS
    "main=nros_ported_main")
```

It also generates `<target>_app_main.c`, which calls `nros_ported_main`.

A NuttX image is not compiled by CMake. Its sources go into the cargo cc-rs
build of `nros-nuttx-ffi`, and the board files carry the definitions over
explicitly:

- `cmake/board/nano-ros-board-qemu-armv7a-nuttx.cmake:364`
- `cmake/board/nano-ros-board-rv-virt-nuttx.cmake:249`

```cmake
get_target_property(_cdefs ${target} COMPILE_DEFINITIONS)
```

That reads the **target's** definitions only. The source-level rename is not
among them, so the application's `main.c` still defines `main`, while the
generated glue that the same link pulls in references `nros_ported_main`.

This is the 0460 shape (a value set on one road and never carried by
another), and the same "NuttX boards ferry by hand" seam that 1115 is about.

## What this is NOT

- **Not the riscv ThreadX argv error** in the same run. That one was
  `rclc_support_init` under GCC 14 and is fixed separately (#2213).
- **Not a flake.** The link is deterministic, and every NuttX C image built
  through `nano_ros_add_executable` should hit it.
- **Not caught by the merge lanes.** No merge-gating lane builds NuttX
  (CLAUDE.md, the 1115 entry), so #2196 merged green.

## What would close this

1. Carry the rename to the NuttX cross-build at both board sites. Either read
   the per-source `COMPILE_DEFINITIONS` of `MAIN_SOURCE` and the extra sources
   and pass them to `nros_nuttx_build_example`, or have `_nros_apply_app_main`
   tell the NuttX road the rename directly. The one rule must not become a
   third spelling.
2. Acceptance is a **build**: `just build-test-fixtures lane=tier2` (or the
   nuttx family alone) links `c_talker` on qemu-armv7a and rv-virt. A
   gate that fails when a board's ferry drops a source-level definition the
   generic road sets would keep it fixed.
