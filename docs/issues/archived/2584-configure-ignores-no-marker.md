---
id: 2584
title: "`nros configure` resolves a package that carries an ignore marker, and the walks spelled the marker list five ways"
status: resolved
type: bug
severity: medium
area: [cli, ux]
related: [phase-478, issue-0809, issue-0621]
found: 2026-11-15
resolved: 2026-11-15
resolved_in: "the change that filed it (the helper and every walk, one change)"
---

**Home: [phase-478](../../roadmap/phase-478-island-developer-experience-findings.md).**
Mirrors GitHub #3871.

## What

`nros configure` resolves the launch tree of every package one level under
`src/`. `scan_workspace` (`packages/cli/nros-cli-core/src/cmd/ws.rs`) feeds
`resolve_system_models`, and it checked no ignore marker. `COLCON_IGNORE`,
`AMENT_IGNORE`, `NROS_IGNORE` and `.nros-ignore` were all ignored there.

The island keeps a contract-only bringup under `src/`: a launch file and a
contract, with no `system.toml` and no nodes. Its contract is meant to fail
the resolver, and it is checked separately with `play_launch check --expect`.
Configure resolved it anyway and refused the whole workspace:

```
Error: refusing to emit a SystemModel: 2 contract error(s)
```

## The class

`cargo-nano-ros`'s `provider_scan::IGNORE_MARKERS` was the full list. Four
other walks spelled their own lists, and three of those were shorter:

| Walk | Honoured before |
| --- | --- |
| `cmd/ws.rs` `scan_workspace` (configure, sync) | none |
| `cmd/bringup.rs` `discover_bringups` | none |
| `cmd/check_workspace.rs` (the root walk) | none |
| `nros-pkg-index` `build_pkg_index` | `COLCON_IGNORE`, `.nros-ignore` |
| `cargo-nano-ros` `package_discovery` | `COLCON_IGNORE` |
| `builder/discover.rs`, `orchestration/prereq_resolve.rs` | all four, as private copies |

## Fix

The list and `is_ignored_dir` now live in `nros-pkg-index`. That is the leaf
crate every walk can reach, including the proc-macro path.
`provider_scan` re-exports both. Every walk above asks the helper, and no
copy of the list is left.

Sweep, to re-run (should print only `nros-pkg-index/src/lib.rs` and test
fixtures):

```
git grep -n '"AMENT_IGNORE"\|"COLCON_IGNORE"\|"NROS_IGNORE"\|"\.nros-ignore"' -- 'packages/**/*.rs'
```

## Decided: no narrowing to image-reached launch trees

The issue offered a second remedy: resolve only the launch trees that a
declared image reaches. That is not done. A workspace with no `system.toml`
anywhere is the ROS-style case, and configure resolves its bringups by
default, so narrowing would stop emitting models users rely on today. The
marker is the opt-out a user already knows from colcon.

## Related

GitHub #3858: the same scan is one level deep only.

## Resolution

Fixed as above. Each walk's own test names every marker:
`every_ignore_marker_hides_a_package` (pkg-index) and
`a_package_with_an_ignore_marker_is_not_scanned` (configure's scan). The
second was mutation-checked: removing the check from `scan_workspace` turns it
red.
