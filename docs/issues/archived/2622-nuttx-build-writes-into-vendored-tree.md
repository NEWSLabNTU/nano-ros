---
id: 2622
title: "nano-ros's NuttX build writes its markers, lock and export snapshots INTO the vendored `third-party/nuttx/nuttx` tree"
status: resolved
resolved_in: "the change that archives this issue"
type: tech-debt
area: [nuttx, build]
severity: low
found: 2026-11-15
related: [phase-550, 2611, 0750]
---

**Owner:** route to phase-550's owner. Found by nros 139 while working on the
issue 2611 follow-up; filed by nros 175.

## What happens

`scripts/nuttx/build-nuttx.sh` runs `cd "$NUTTX_DIR"` (the vendored
submodule, `third-party/nuttx/nuttx` by default) and writes nano-ros's own
state at that tree's root:

| Path | Writer (`build-nuttx.sh`) | What it is |
|---|---|---|
| `.nros-nuttx-build.lock` | line 133, `exec 9>` | the build's flock |
| `.nros-nuttx-build-head-<config id>` | line 195 (`MARKER`), written at 328 | the `HEAD:sha256(defconfig)` freshness key |
| `nros-nuttx-export-<config id>/` | line 192 (`NUTTX_SNAPSHOT`) | the per-config export snapshot (issue 0750 B) |

The main checkout's tree on newslab-175 holds six such paths:
`.nros-nuttx-build-head`, `-arm`, `-riscv`, `.nros-nuttx-build.lock`,
`nros-nuttx-export-arm/` and `nros-nuttx-export-riscv/`. `just/nuttx.just`
adds `nros-nuttx-export-arm-smp/` through the same script.

NuttX's `.gitignore` does not cover them, so the submodule reads as dirty
(`??`) in every checkout that has built NuttX. CLAUDE.md says not to modify
vendored trees, and this is a standing exception nobody chose. It has
already cost something: issue 2611's `worktree-prune` read these files as
unsaved work and refused to remove merged worktrees. It now carries an
explicit allow-list for them, `NROS_GENERATED_IN_NESTED` in
`scripts/worktree-prune.py`. Keep it until this issue is fixed.

NuttX's OWN in-tree kernel build output (`.config`, objects, `System.map`,
`Make.dep`, and so on) is a different matter. NuttX ignores it, and building
in-tree is how NuttX's make build works (phase-550 builds on NuttX's own
infrastructure). This issue is only about the files nano-ros adds.

## Expected

nano-ros's markers, lock and export snapshots live under a nano-ros build
directory outside the submodule, keyed as they are now by config id.
`git -C third-party/nuttx/nuttx status --porcelain` after a NuttX build then
prints nothing.

## Where to look first

The writer is `scripts/nuttx/build-nuttx.sh`. Several readers derive the
snapshot path as `<NUTTX_DIR>/nros-nuttx-export-<id>`, and they must move in
the SAME change, through one locator rather than five spellings:

- `packages/api/nros-c/cmake/nros-nuttx.cmake` (`_nnbe_export_dir`, around
  line 560)
- `cmake/platform/nano-ros-nuttx.cmake:121`
- `packages/boards/nros-board-common/src/nuttx_export.rs` (lines 65, 178)
- `packages/cli/nros-cli-core/src/cmd/build.rs` (around line 9463)
- `packages/testing/nros-tests/src/fixtures/binaries/nuttx.rs`
- `integrations/nuttx/Makefile:268` (comment) and `just/nuttx.just`

`NUTTX_EXPORT_DIR` already overrides the location on the cmake road, so a
default under the build root may need only the default to change. Measure
that before relying on it.

When this lands, delete the `third-party/nuttx/nuttx` row of
`NROS_GENERATED_IN_NESTED` and its self-test cases in the same PR. Its
comment cites `nros-nuttx.cmake` and `just/nuttx.just` as the producers;
the writer is `build-nuttx.sh`. Correct that if the allow-list outlives this
fix.

## Fix

nano-ros's state for a NuttX tree now lives in `<tree>.nros-build/`, a
SIBLING of the tree: `build.lock`, `build-head-<config id>` and
`nros-nuttx-export-<config id>/`. It sits beside the tree and not under a
checkout's build root, because it describes that tree's in-tree kernel
build, which every checkout sharing the tree shares. One locator per
language, and every reader goes through it:

- shell: `scripts/lib/nuttx-state.sh` (`nros_nuttx_state_dir`,
  `nros_nuttx_export_snapshot`), used by `build-nuttx.sh`,
  `check-sched-dim-arms-compile.sh` and `just/nuttx.just`;
- cmake: `nros_nuttx_export_snapshot` in `nros-nuttx.cmake`, used by it and
  by `nano-ros-nuttx.cmake`;
- Rust: `nros_build_paths::nuttx_export_snapshot` / `nuttx_state_dir`, used
  by `nuttx_include_root`, `nros-board-common`'s `nuttx_export`, the CLI's
  `nuttx_cpu_config_path` and `nros-tests`' `nuttx_kernel_path_for`.

`NUTTX_EXPORT_DIR` still overrides the snapshot on every road, unchanged.

`build-nuttx.sh` removes, under its lock, what an earlier build left IN the
tree. It removes exact names only, and only paths the tree's git does not
track (`nros_nuttx_retire_in_tree_state`). `.gitignore` covers
`/third-party/nuttx/*.nros-build/`.

`worktree-prune`'s `NROS_GENERATED_IN_NESTED` allow-list and its matcher are
gone: an untracked path in a nested repo is a refusal again, with nothing
excused by name. Its self-test keeps the cases that still mean something.
NuttX's own ignored output is clean, and a user's file is refused. A new case
shows an old in-tree lock is no longer excused.

Gate `check-nuttx-state-outside-tree` (fast line) checks two things. No code
line outside the three locators spells a snapshot or marker name; on main it
flags 20 lines. The shell and cmake locators agree, through `cmake -P`.
`nuttx_state_lives_beside_the_tree` pins the Rust one.

Measured (2026-11-19, newslab-118): a real `build-nuttx.sh` run (arm) in a
worktree whose NuttX submodule held the three old in-tree paths. It exited
0. Afterwards `git -C third-party/nuttx/nuttx status --porcelain` printed
nothing, the superproject's status for `third-party/nuttx` printed nothing,
and `nuttx.nros-build/` held `build.lock`, `build-head-arm` and
`nros-nuttx-export-arm/`. A second run took the up-to-date short-circuit
from the new snapshot.

Sweep: `git grep -nE 'nros-nuttx-export-|[.]nros-nuttx-build' -- ':!docs'`
