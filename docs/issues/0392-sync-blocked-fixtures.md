---
id: 392
title: Six bringups cannot `nros sync` at all — two legacy `system.toml` schemas,
  two components with no `class`, one launch needing an uninstalled package
status: open  # A, B and the metadata-path finding fixed 2026-08-02; C remains
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

## Status — A and B fixed (2026-08-02)

| Case | Was | Now |
| --- | --- | --- |
| `multi_pkg_workspace_freertos` (A) | parse error | **syncs, rc=0** |
| `n9_workspace` (A) | parse error | **parses; model regenerates correctly** |
| `multi_pkg_workspace_nuttx` (B) | harness failure | **syncs, rc=0** |
| `orchestration_e2e` (B) | harness failure | **syncs, rc=0** |

All regenerate models that differ from the committed copies only in the
known-stale `scope` field.

**A — the schema needs three fields, and reports them one at a time.**
`n9_workspace` was missing `name`, `rmw` AND `domain_id`; each attempt named
only the next one, so the gap revealed itself one line per run.
`multi_pkg_workspace_freertos` was migrated off the phase-212 spelling
(`launch` → `default_launch`, `zenoh_locator` → `locator`, `components`
dropped — the node set comes from the launch file).

**The `NROS_SYSTEM_TOML` warning in this issue was WRONG.** I wrote that a BSP
`build.rs` might depend on the legacy spelling. Grepping the tree for
`NROS_SYSTEM_TOML` returns NOTHING — that reader no longer exists, and the
comment claiming it does is itself stale. The migration had no second consumer
to break.

**B — two different causes under one error message.**

  * `multi_pkg_workspace_nuttx`'s packages are C-ABI STUBS
    (`#[no_mangle] nros_node_listener`) that declared
    `[package.metadata.nros.node]`. There is no Rust type for `class` to name,
    so the block was removed — it only restated the default namespace anyway,
    and the `multi_pkg_workspace_freertos` siblings that sync clean declare no
    such block.
  * `orchestration_e2e/demo_pkg` has a REAL component
    (`demo_pkg::talker::Component`) and no way to say so: the legacy whole-file
    manifest schema had no `class` field, and `Workspace::discover` hardcoded
    `class: None`. Fixed in code — `ComponentConfig` gains `class` and discover
    honours it — plus the declaration in the fixture. The Cargo-metadata form
    has carried `class` since phase-307 W1; only the legacy form could not
    express it.

## New finding — FIXED 2026-08-02

`source.artifact` is now recorded relative to the component package
(`src/lib.rs`), so a sync no longer rewrites the tracked file with the syncing
user's home directory. Normalisation happens where the harness output lands, in
`build_metadata`.

Two approaches were possible and the choice matters:

  * `--remap-path-prefix` via `RUSTFLAGS` would fix it at the source, but that
    env var REPLACES any `[build] rustflags` from the workspace's
    `.cargo/config.toml` — which the embedded packages here depend on. Rejected.
  * a textual prefix strip on the emitted JSON, chosen, and deliberately NOT a
    parse-and-reserialise: reserialising would silently reformat every generated
    metadata file the first time it ran.

**Separately, regeneration shows the committed `talker.json` is STALE** — 122
inserted lines, and `id` moves `node_talker` → `talker`. That is a real refresh,
not a path issue, and it is test-visible, so it was left out of the path fix.

## The original finding — `nros sync` writes host-absolute paths into tracked metadata

Making `orchestration_e2e` syncable exposed the next problem: sync regenerates
`src/demo_pkg/metadata/talker.json` with

```
"artifact": "/home/aeon/repos/nano-ros/packages/cli/testing_workspaces/…/src/lib.rs"
```

which `check-absolute-paths` rejects. The file is TRACKED, so anyone who syncs
that workspace dirties the tree with their own home directory. This is the
issue-0320 class one layer over: models were taught to record relative paths via
`--bringup-root`, the metadata writer never was. The regenerated file was
restored rather than committed.

## Direction

A and B are done. What remains:

  * **C** — `o5_nav2_compat_smoke` needs `secondary_node` built and sourced
    before its launch resolves. Arguably not a defect; decide whether sync
    should be runnable on it at all.
  * **refresh the stale `orchestration_e2e` metadata** — regeneration produces
    122 more lines and renames a node id; needs its tests run, so it is its own
    change.
  * `n9_workspace` still cannot complete a sync IN PLACE, but only because of
    the `@NANO_ROS_ROOT@` template placeholder documented below — its own
    schema defect is fixed.
