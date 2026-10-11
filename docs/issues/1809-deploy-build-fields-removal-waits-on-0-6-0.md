---
id: 1809
title: "`[deploy.*]` build fields are deprecated but still parsed; removal waits on the 0.6.0 release"
status: open
type: tech-debt
area: [cli, orchestration]
severity: low
found: 2026-10-11
related: [0951]
---

## Measured

[phase-383](../roadmap/archived/phase-383-colcon-like-builder.md) moved every
build fact into `[image.*]` (W1-W9, W10.a/c/d). Its W1.f deprecated the build
fields inside `[deploy.*]` (`board`, `target`, `rmw`) and promised removal at the
next minor version. That last step, W10.b, is the only item left, and the phase
was archived on 2026-10-11 with it moved here.

Re-measured 2026-10-12 on main:

1. **The tree is already migrated.** No tracked `system.toml` has a `[deploy.*]`
   block. Issue 0951 took it from 96 blocks to 0 on 2026-08-31 (`eb20898644`).
   Placement went to `[host.<name>]`, build facts to `[image.<id>]` and site
   config to `[board_config.<board>]`. The "42 build fields in about 14 blocks"
   phase-383 recorded is no longer true.
2. **The support code is still live, for out-of-tree files.**
   `SystemToml::deploy` is still parsed (`cargo_metadata_schema.rs`), and the lint
   `deprecated_deploy_build_field_warnings` (`orchestration/image.rs`) is still
   reached from `collect_images` in `cmd/build.rs`. The deploy arm of
   `SystemToml::resolved_rmw` is still consulted below an `[image.*].rmw`.
3. **The version boundary has not been crossed.** The workspace is `0.5.0`.
   Bumping the minor version is a release decision, not a migration step.

## What 0.6.0 must do

Delete the lint (`deprecated_deploy_build_field_warnings`), its call in
`collect_images`, the deploy arm of `resolved_rmw` and its precedence test, and
then the `deploy` table in the schema, so that an old `[deploy.*]` block is
refused by name rather than silently ignored. Nothing in-tree needs migrating
first, which `git grep -n '^\[deploy' -- '*.toml'` (empty) confirms.
