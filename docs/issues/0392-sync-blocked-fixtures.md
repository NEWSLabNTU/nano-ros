---
id: 392
title: Six bringups cannot `nros sync` at all — two legacy `system.toml` schemas,
  two components with no `class`, one launch needing an uninstalled package
status: open
type: bug
area: testing
related: [rfc-0063, phase-330, 0387, 0380]
---

## Problem

Six bringups fail `nros sync` outright. Not "produce a different model" — fail,
before writing anything.

This matters beyond the fixtures themselves because phase-330 W4 deletes the
committed `system_model.yaml` files and regenerates them at build time. A
bringup that cannot sync cannot regenerate, so each of these is a W4.a blocker.

Found by the phase-330 W4.a dry run (move `config/` aside, regenerate from
inputs alone, restore) across all 76 bringups. Twelve failed; six of those were
artifacts of the probe or expected, and are documented at the bottom so nobody
re-investigates them.

## The six

### A. `[system]` does not satisfy the current schema (3 bringups, 2 files)

`packages/testing/nros-tests/fixtures/n9_workspace/src/demo_bringup/system.toml`

```
Caused by:
    6 | [system]
      | ^^^^^^^^
    missing field `name`
```

It declares only `default_launch`. Its header comment describes
`nros::main!(launch = "demo_bringup")` — a form phase-296 R4 REMOVED — so the
file predates both the current schema and the current macro surface.

`packages/testing/nros-tests/fixtures/multi_pkg_workspace_freertos/src/demo_bringup/system.toml`
(fails for both the `firmware` and `src/demo_bringup` bringups, same file)

```
Caused by:
    10 | launch     = "launch/system.launch.xml"
       | ^^^^^^
    unknown field `launch`, expected one of `name`, `rmw`, `domain_id`,
    `ros_edition`, `locator`, `default_launch`, `default_target`, `features`
```

It uses `launch`, `components` and `zenoh_locator` — an older spelling
throughout. Its header says the BSP's `build.rs` reads the file via
`NROS_SYSTEM_TOML`, so **a second reader may depend on the legacy spelling**.
Do not migrate it blind: find that reader first, or the fix trades a sync
failure for a FreeRTOS build failure.

Both fixtures are live: `n9_workspace` has 4 mentions in `examples/fixtures.toml`
and `multi_pkg_workspace_freertos` has 1.

### B. A component declares no `class` (2 bringups)

`packages/cli/testing_workspaces/orchestration_e2e/src/demo_pkg` and
`packages/testing/nros-tests/fixtures/multi_pkg_workspace_nuttx/src/demo_bringup`:

```
component 'talker' declares no `class` and its id is not `crate::module`
  — cannot name the registered type
    at nros-cli-core/src/orchestration/metadata_build.rs:252
```

Needs a decision before a fix: is this a genuine defect in the fixtures, or are
they NEGATIVE fixtures that exist to exercise this very error path? The error is
raised deliberately and reads like a designed check.

### C. A launch file needs a package that is not installed (1 bringup)

`packages/testing/nros-tests/fixtures/o5_nav2_compat_smoke/demo_entry`:

```
Invalid substitution syntax: Package 'secondary_node' not found.
Ensure the package is installed and sourced.
```

Environment-dependent, not a model defect — the launch resolves a package that
must be built and sourced first. The open question is whether sync should be
runnable on this fixture at all, or only after its workspace is built.

## Not bugs — recorded so they are not re-investigated

**Three were the probe's fault, not the tree's.** `bins/entry-poc`,
`bins/qemu-baremetal-main-e2e` and `fixtures/n_board_agnostic_run_plan` reported
"no `src/<pkg>/package.xml` and no `package.xml` at root". The sweep script
derived the workspace root by walking up one level, which lands in `bins/` or
`fixtures/` for a single-package tree. Running sync in the correct directory
gives `rc=0` for both spot-checks.

**Three are unsubstituted templates, and that is by design.**
`o4_pkg_index_workspace`, `orchestration_tiers_native` and
`orchestration_tiers_freertos` fail with

```
unable to update …/src/ctrl_pkg/@NANO_ROS_ROOT@/packages/api/nros
```

`@NANO_ROS_ROOT@` is a `configure_file` placeholder: these fixture sources are
templates materialised into a build directory before use, so syncing the
template in place cannot work. Sync them post-configure, or not at all.

## Direction

A and B are the ones that need a decision. C is arguably not a defect. Whoever
takes A must trace the `NROS_SYSTEM_TOML` reader before touching the FreeRTOS
descriptor — the sync failure is the visible symptom, and the second consumer is
the risk.
