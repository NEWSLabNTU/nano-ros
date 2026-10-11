---
id: 1813
title: "A single-package leaf's bringup is still named twice: by directory in `nros sync`, by `[system] name` in the cmake metadata step"
status: open
type: tech-debt
area: [cli, cmake, orchestration]
severity: low
found: 2026-10-11
related: [1296, 1308, 1305]
---

## Measured

[Issue 1296](archived/1296-nros-build-c-leaf-bringup-name-mismatch.md) found
that a single-package C/C++ leaf's synthetic bringup is derived from two
different facts:

- `synthesise_leaf_launch` (`packages/cli/nros-cli-core/src/cmd/ws.rs:1840`)
  names the launch directory after the leaf's DIRECTORY
  (`<leaf>/build/nros/launch/<leaf-dir>/system.launch.xml`), and the model
  lands under `models/<leaf-dir>/`.
- `cmake/nano_ros_workspace_metadata.cmake` (lines 35-42, 61) asks `nros plan`
  for `${_NRW_SYSTEM}`, the leaf's `[system] name`.

For `examples/native/c/talker` those are `talker` and `native_c_talker`.

Commit `754021a407` resolved 1296 and 1308 by giving `nros build` its own road
for such a leaf (`plan_single_package_cmake`), which configures the leaf's own
`CMakeLists.txt` and never reaches the workspace metadata step. That fixed the
user-visible failure, and the acceptance builds pass. It did not make the two
derivations agree, and 1296's own Fix asked for one helper plus a sweep of
every consumer of the bringup name. Neither was done.

## Why it still matters

Nothing in-tree reaches the mismatch today. Anything that later asks `nros plan`
(or reads `models/<name>/`) about a single-package leaf by its `[system] name`
gets "no SystemModel" again, with a remedy (`nros sync`) that was just run. The
same shape cost 1296 a dead-end error message.

## Fix

One function names a leaf's bringup, and both sides call it (the cmake side
through the CLI, never by re-deriving). Then sweep the consumers:
`git grep -n "_NRW_SYSTEM\|models/\|launch/system.launch.xml" -- cmake packages/cli`.
Acceptance: a leaf whose directory name differs from its `[system] name` gives
the same model path from `nros sync` and from `nros plan <[system] name>`.
