---
id: 2052
title: "`check pristine-checkout` reaches a stale `~/.nros/bin/nros` through `cmake-ament-surface`"
status: resolved
type: bug
area: [ci, cmake, cli]
severity: medium
found: 2026-10-25
resolved: 2026-10-26
related: [phase-505, issue-0726, issue-1280, issue-1373]
---

**Home:** [phase-505](../../roadmap/archived/phase-505-ci-lanes-reach-and-infra-robustness.md) W2, since 2026-10-26.

## What

`just ci gate` step 4, `check::pristine-checkout`, runs the fast tier in a
fresh clone with no submodules and no built CLI. Since
`c927d97b49` (phase-488, "the ament verbs take a stock ROS 2 package's
CMakeLists") `cmake-ament-surface`'s configures reach
`NanoRosGenerateInterfaces.cmake` → `nros_resolve_cargo_profile`, which needs
`nros profile`. The clone has no `packages/cli/target/release/nros`, so
`_nros_resolve_codegen_tool` falls through to `~/.nros/bin/nros` — on the
host this was measured on, a May 2026 `nros 0.3.6` with no `profile`
subcommand:

```
-- Found nros codegen tool: /home/aeon/.nros/bin/nros
CMake Error at …/cmake/NanoRosCargoProfile.cmake:63 (message):
  nano-ros: error: unrecognized subcommand 'profile'
```

The same gate is green in the worktree itself (`just check
cmake-ament-surface`: OK), where the in-tree CLI is on `PATH`.

## Why it matters

The gate's own rule (issues 1373/1345/1043) is that a pristine checkout's red
must be NOT VERIFIED with a remedy, never a FAIL, and the subject here — a
binary outside any checkout — is not even the tree under test. It also
reproduces the 0726 class: a stale `~/.nros/bin` binary answering for the
checkout. A host with no `~/.nros/bin/nros` instead hits the resolver's
`FATAL_ERROR`, which is a FAIL for a different wrong reason.

## Fix shape

`cmake-ament-surface` (or the pristine harness) must hand its configures the
checkout's own CLI, or report NOT VERIFIED when the clone has none — never let
`find_program` reach a binary nobody chose for this tree.

## Resolution (2026-10-26)

Fixed by `8b95e90dd6` ("cmake-ament-surface uses this checkout's CLI, or
skips"), landed on main at 2026-10-26 03:25Z without citing this issue, so the
record stayed open and was homed in phase-505 later that day.

`just/check/codegen.just` resolves `nros_cli_bin --checkout-only`, reports NOT
VERIFIED through the skip ledger when the checkout has none, and passes it to
`tests/cmake-ament-surface-tests.sh` as `NROS_CLI`; the script pins it the
same way when run directly. The shape is issue 1850's
(`bounds-fragment-configure`).

Re-measured 2026-10-27 in a fresh worktree of main with no built CLI and a
stand-in `nros` that refuses `profile` first on `PATH`:

```
$ PATH=<stale>:$PATH just check cmake-ament-surface
[SKIPPED] cmake-ament-surface: no in-tree CLI -- run `just setup-cli`
```
