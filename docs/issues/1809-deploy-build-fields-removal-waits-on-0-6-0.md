---
id: 1809
title: "`[deploy.*]` build fields are deprecated but still parsed; removal waits on the 0.6.0 release"
status: open
type: tech-debt
area: [cli, orchestration]
severity: low
found: 2026-10-11
related: [0951, phase-486]
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

## Narrowed 2026-10-13 by phase-486 W8a

Part of this landed early, because rlm stopped reading `[deploy.*]` (phase-486
W8, rlm is a generic system-config reader). The fields that reached an image
only through rlm's placement — `nodes`, `launch`, `domain_id`, `locator` and
`rmw` — would otherwise have been dropped silently, so the parse now REFUSES
them by name, naming `[host.*]` / `[image.*]`. With them went the deploy rungs
of `resolved_rmw`, `resolved_domain_id` and `resolved_locator` and their
tests, the `[deploy.<t>] launch` fallback in `codegen-system`, and doctor's
deploy launch check. The type is nano-ros's own now (`DeployTarget` in
`cargo_metadata_schema.rs`), no longer a re-export of rlm's `DeployBlock`.

What still parses with the deprecation warning, until 0.6.0: the build fields
(`kind`, `target`, `board`, `framework`, `profile`, `optimize`, `features`,
the `[deploy.<n>.nros]` sub-table) and `[param_services]`.

## What 0.6.0 must do

Delete the lint (`deprecated_deploy_build_field_warnings`), its call in
`collect_images`, the remaining `[deploy.*]` readers (`resolve_image`'s sole-
deploy rung, `board_facts`' deploy candidates, `doctor`'s deploy listing), then
the `deploy` table and `DeployTarget` in the schema, so that an old
`[deploy.*]` block is refused by name rather than silently ignored. The same
for `[param_services]`, naming `features = ["param_services"]`. Nothing in-tree needs migrating
first, which `git grep -n '^\[deploy' -- '*.toml'` (empty) confirms.

## Ready, held for the 0.6.0 bump (2026-10-13)

Implemented on branch `feat/1809-deploy-removal`, on top of phase-486 W8a.
HELD by maintainer decision (2026-10-12): it is a breaking change for an
out-of-tree `system.toml` that still writes `[deploy.*]` or
`[param_services]`, so it lands with the 0.6.0 version bump and this issue
stays open until then. Nothing is lost silently: 0 tracked `system.toml`
carry either table, and an out-of-tree one is REFUSED by name.

- `SystemToml::deploy`, `DeployTarget` and W8a's `refuse_rlm_placed` are gone.
  A `[deploy]` or `[param_services]` key is refused by the strict schema with
  `nros_orchestration_ir::retired_tables`' message, which names `[image.<id>]`
  and `[host.<name>]` (or `features = ["param_services"]`). The overlay, the
  leaf reader, `leaf_capabilities` and `nros::main!` ask the same helper.
- Deleted with their tests: the lint (`DEPRECATED_DEPLOY_FIELDS`,
  `deprecated_deploy_build_field_warnings`) and its `collect_images` call,
  `resolve_image`'s sole-deploy rung, `target_board_id`'s `kind` rung, the
  planner's build tuning, `board_facts`' deploy candidates, `check`'s and
  `config`'s deploy reports, codegen-system's `--target` block check, and the
  self-bringup synthesis's `[deploy.<t>]` rows (rows that set different session
  values are refused, `SelfBringupSessionDisagrees`).
- `nros doctor` lists `[host.*]` instead of deploy targets and still reports a
  file that does not parse (W8a's rule), so a retired table is a problem, not
  a skip.
- `DeployTargetMetadata` (Cargo `[package.metadata.nros.deploy.*]`) and
  `board-facts --deploy` are other surfaces and stay.

To land: rebase onto the 0.6.0 bump, re-run `just ci gate`, then archive this
issue.
