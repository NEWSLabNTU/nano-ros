---
id: 2622
title: "nano-ros's NuttX build writes its markers, lock and export snapshots INTO the vendored `third-party/nuttx/nuttx` tree"
status: open
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
