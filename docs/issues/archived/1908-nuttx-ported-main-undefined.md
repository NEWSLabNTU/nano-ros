---
id: 1908
title: "NuttX images fail to link `nros_ported_main`: phase-499 W5's per-SOURCE `main` rename never reaches the cargo cross-build"
status: resolved
resolved: 2026-10-15
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

## Resolved (2026-10-15)

**The fix** landed in `eadf501a4c` (filed under #1907): one helper,
`nros_target_ferried_compile_definitions` (`cmake/NanoRosVerbs.cmake`), returns
the target's `COMPILE_DEFINITIONS` unioned with every source's, and both NuttX
boards call it. That commit verified it on the nine NuttX `rtos_e2e` cells.

**The acceptance build**, measured separately on `origin/main` `a39b85c5de`:
`just nuttx build-fixtures` (arm and riscv) exits 0, and both C talkers link
with the rename applied:

| image | `nros_app_main` | `nros_ported_main` |
| --- | --- | --- |
| `qemu-armv7a-nuttx/c/talker` | `T 00631d98` | `T 00631e50` |
| `rv-virt-nuttx/c/talker` | `T 8002b7fa` | `T 8002b884` |

**The gate** item 2 asked for is `check-cmake-ferried-compile-definitions`
(fast line). It refuses any read of a target's `COMPILE_DEFINITIONS` outside
that helper: `get_target_property(… COMPILE_DEFINITIONS)`,
`get_property(… TARGET … PROPERTY COMPILE_DEFINITIONS)` or
`$<TARGET_PROPERTY:…,COMPILE_DEFINITIONS>`. Those reads are what dropped the
per-source rename, and the next board ported to an out-of-CMake build would
copy them. `INTERFACE_COMPILE_DEFINITIONS` and a SOURCE's definitions are not
refused. Run against the pre-fix `nano-ros-board-rv-virt-nuttx.cmake`, it names
line 249, the `get_target_property(_cdefs …)` this issue quoted. Its self-test
holds ten cases on every run.

This is also the "NuttX cc-rs gap": a board that compiles a target's sources
in cargo's cc-rs carries only what it is handed.
