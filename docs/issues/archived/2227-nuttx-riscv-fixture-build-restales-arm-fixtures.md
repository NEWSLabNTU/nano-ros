---
id: 2227
title: "`just nuttx build-fixtures` re-stales its own arm fixtures: the riscv half rewrites the shared tree's `include/nuttx/config.h`"
status: resolved
type: bug
severity: medium
area: [build, nuttx, testing]
related: [0433, 0525, 1007]
found: 2026-11-03
---

## What

`just nuttx build-fixtures` builds arm, then riscv. Both reconfigure the one
checkout at `third-party/nuttx/nuttx`, and the riscv configure rewrites
`include/nuttx/config.h`. Every arm Rust fixture built before it then resolves
as STALE:

```
Test fixture is STALE — a source is newer than the built binary:
  binary: build/cargo-fixtures/nuttx/armv7a-nuttx-eabihf/nros-minsizerel/talker
  newer:  third-party/nuttx/nuttx/include/nuttx/config.h
```

Timestamps from one family build: talker 16:29:22, `config.h` 16:46:21. The
staleness probe exempts one regenerated-in-place header; `config.h` is not it.

## Measured

After `just nuttx build-fixtures` (rc=0), six NuttX arm tests in the issue 2221
run failed STALE (`test_rtos_{pubsub,service,action}_e2e` Rust, `quiet_link`,
`nuttx_arm_carves_the_parameter_store_and_delivers`). After
`just nuttx build-fixtures-arm` alone, the same NuttX lanes ran 17 of 17 green.

So the family build cannot produce a fresh arm set. This is the
"last configured wins" class of issues 0433/0525 (both archived), reached by a
newer path: the riscv half of the same recipe.

## Resolution

The fixtures never read the shared header. They compile against the arm
snapshot (`nros-nuttx-export-arm/include`). Two build scripts nevertheless
emitted `cargo:rerun-if-changed` on the SHARED `$NUTTX_DIR/include/nuttx/config.h`
unconditionally, and cargo writes every build-script watch into the binary's
dep-info, which is what the staleness probe reads:

- `nros_build_paths::nuttx_include_root` watched the shared header on every
  branch, including the one where the snapshot won.
- `nros_board_common::nuttx_image_link` watched it again, with a comment citing
  issue 0477.

Issue 0477's rule is narrower than both: watch the PREFERRED path even when it
loses, so an artifact built from the fallback rebuilds when the snapshot
appears. It never asked to watch the fallback when the preferred path wins. Now
`nuttx_include_root` watches the shared header only when it returns it, and the
image-link script relies on `include_root`'s edge instead of its own.

Measured in a fresh worktree:
- After `just nuttx build-fixtures` (arm then riscv), the arm talker's dep-info
  names `nros-nuttx-export-arm/include/nuttx/config.h` and not the shared header
  (`talker.d`, `service-server.d`, `action-server.d`: 0 occurrences).
- `touch third-party/nuttx/nuttx/include/nuttx/config.h` (what a riscv
  reconfigure does), then the NuttX arm lanes: 11 of 11 pass, none STALE.
  Before the fix, the same state turned six of them red.

Sweep: `git grep -nE 'include/nuttx/config\.h' -- 'packages/**/*.rs'`, which now
finds no build-script watch on the shared header.
