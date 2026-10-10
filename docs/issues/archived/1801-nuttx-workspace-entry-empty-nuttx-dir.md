---
id: 1801
title: "A NuttX WORKSPACE entry built with `NUTTX_DIR` empty — `cmake -E chdir \"\" bash …` — because the board module locates the tree inside the nano-ros subdirectory scope"
status: resolved
type: bug
severity: medium
area: [build, cmake, nuttx]
related: [phase-484, issue-1280, issue-1795]
found: 2026-10-11
---

## Measured

Found while verifying issue 1795 in a fresh worktree. `just nuttx
build-fixtures-arm` built every example leaf, then failed on the
`examples/workspaces/c` NuttX entry:

```
[100%] Building NuttX example: nuttx_entry
CMake Error: Directory does not exist for chdir command: bash
gmake[3]: *** [.../nuttx_entry_build.dir/build.make:113: cargo-target/armv7a-nuttx-eabihf/nros-minsizerel/nros-nuttx-ffi] Error 1
```

The generated rule read `cmake -E env NUTTX_DIR= … cmake -E chdir bash …`:
`NUTTX_DIR` was empty, so the directory argument vanished and `chdir` took
`bash` as its directory. `nros locate` itself answered correctly
(`set(NROS_LOCATE_ENV_NUTTX_DIR ".../third-party/nuttx/nuttx")`).

Reconfiguring that build dir with the branch's pre-1795 cmake gave the same
empty value. So this predates issue 1795, and `main` carries the same code.

## Cause

Phase-484 W2c moved the NuttX board modules to
`nros_locate_var(NUTTX_DIR)`. That function sets a **normal** variable by
design: `NanoRosLocate.cmake` keeps locations out of the cache so that
dropping an override takes effect on the next configure.

The board module runs inside `add_subdirectory(<nano-ros root>)`. So the
located value lives in that subdirectory's scope, while a workspace entry's
image is declared, and `nros_nuttx_build_example` runs, in the workspace's own
directory, where `NUTTX_DIR` is unset.

An example leaf never meets this, because its recipe passes `-DNUTTX_DIR` as a
cache entry, which every scope sees. The default-path branch in the board
module also sets a cache entry. Only a located value, which is the normal
case since W2c, was invisible to the workspace road.

The `nros_locate_var` callers for ThreadX and FreeRTOS are consumed by
library targets inside the subdirectory, so only the NuttX image build, which
runs from the caller's scope, read it from the wrong scope.

## Fixed

`nros_nuttx_build_example` (`packages/api/nros-c/cmake/nros-nuttx.cmake`)
now asks `nros_locate_var(NUTTX_DIR)` and `nros_locate_var(NUTTX_APPS_DIR)`
in its own scope whenever they are empty. The located answers are GLOBAL
properties, so they are visible from any directory, and an explicit `-D` still
wins inside `nros_locate_var`. This keeps NanoRosLocate's "normal variables,
never CACHE" design. If `NUTTX_DIR` is still empty or not a directory, the
function now stops with `FATAL_ERROR` and names `just nuttx setup`, where it
used to emit a command whose failure said nothing about the cause.

## Evidence

- Reconfiguring the same build dir after the fix gave
  `env NUTTX_DIR=<worktree>/third-party/nuttx/nuttx`.
- After the fix, `just nuttx build-fixtures-arm` builds every arm row,
  including the workspace entry and issue 1795's NuttX typed carrier.
