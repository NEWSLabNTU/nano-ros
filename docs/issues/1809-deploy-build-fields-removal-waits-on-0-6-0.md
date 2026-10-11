---
id: 1809
title: "`[deploy.*]` build fields are deprecated but still parsed; removal waits on the 0.6.0 release"
status: open
type: tech-debt
area: [cli, orchestration]
severity: low
found: 2026-10-11
related: []
---

## Measured

[phase-383](../roadmap/archived/phase-383-colcon-like-builder.md) moved every
build fact into `[image.*]` (W1-W9, W10.a/c/d). Its W1.f deprecated the build
fields inside `[deploy.*]` (`board`, `target`, `rmw`) and promised removal at the
next minor version. That last step, W10.b, is the only item left, and the phase
was archived on 2026-10-11 with it moved here.

State as phase-383 last measured it (2026-08-31):

1. **The warning is emitted.** `deprecated_deploy_build_field_warnings` is
   reached from `collect_images` (`packages/cli/nros-cli-core/src/cmd/build.rs`),
   which every `nros build` passes through. A wiring test asserts it. The
   deprecation period started when that landed, not at `0.5.0`.
2. **The version boundary has not been crossed.** The workspace is `0.5.0`.
   Bumping the minor version is a release decision, not a migration step.
3. **42 build fields remain, in about 14 deploy blocks, and none has a matching
   `[image.*]`.** The 20 that duplicated an image key were removed (dry-run
   output byte-identical before and after). Migrating the rest means creating
   new image blocks, each a new unit `nros build --all` would build. Several
   are placement-only deploys (`robot1` / `robot2` with a host `target`), where
   whether an image should exist is a design question.
4. **`[deploy.*].rmw` is still read.** `SystemToml::resolved_rmw` is called with
   a real deploy target from `codegen_system.rs` and `planner.rs`
   (`nros plan --target`), where an `[image.*].rmw` now outranks it.

## What 0.6.0 must do

Decide, per remaining deploy block, whether it becomes an image or loses its
build fields. Then delete the lint, `DEPRECATED_DEPLOY_BUILD_FIELDS`, the
wiring in `collect_images`, and the deploy arm of `resolved_rmw`.

Re-count first. Phase-383's count went from 37 to 56 between two measurements,
so treat the numbers above as a starting point only.
