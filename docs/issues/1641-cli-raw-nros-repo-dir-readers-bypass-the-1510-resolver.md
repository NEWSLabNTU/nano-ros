---
id: 1641
title: "Seven CLI-side readers take `$NROS_REPO_DIR` raw and bypass issue 1510's
  resolver, so from a linked worktree they act on the PARENT checkout — one
  writes a file into it, and one emits path deps that compile its crates"
status: open
type: bug
area: [cli, build]
severity: medium
found: 2026-10-02
related: [1510, 1538, 1280, 1391, 0196, 0409]
---

## What happens

Issue 1510 fixed the rule for the CLI's SDK-root ladder in one place —
`orchestration/nano_ros_root.rs`, whose `resolve(explicit, workspace)` puts a
walk-up from the workspace ABOVE `$NROS_REPO_DIR`, because an exported variable
cannot distinguish intent from inheritance and a linked worktree inherits the
parent shell's value. Issue 1538 fixed a second reader in
`nros-cbindgen-headers`.

The sweep 1538 asked for (`git grep -n 'NROS_REPO_DIR' -- '*.rs'`) finds **seven
more** that read the variable raw and never reach either fix:

| site | what it does with the value | shape |
| --- | --- | --- |
| `nros-cli-core/src/cmd/build.rs:1144` | SDK root for a single-package build: `--nano-ros-path` > **env** > walk-up | drop-in for `nano_ros_root::resolve` |
| `nros-cli-core/src/cmd/leaf_system.rs:115` | SDK root for a leaf's board catalog and settings: same ladder | drop-in for `nano_ros_root::resolve` |
| `nros-cli-core/src/cmd/ws.rs:4004` | `run_central_patch`: `--nano-ros-path` > **env** > `cwd`, then **writes** the central patch file into that root | needs a decision: its fallback is `cwd` itself, not a walk-up |
| `nros-cli-core/src/cmd/ws.rs:2539` | `resolver_beside`: locates the `nros-launch-resolve` binary | no workspace in hand |
| `nros-cli-core/src/orchestration/sdk_store.rs:766` | reads the Rust channel pin from the repo | no workspace in hand; low impact |
| `rosidl-bindgen/src/generator.rs:686` | emits `{ path = "<root>/packages/core/…" }` deps into GENERATED manifests | lives below `nros-cli-core` |
| `nros-orchestration-ir/src/model_location.rs:316` | locates the `nros-launch-resolve` binary | a core crate; cannot depend on the CLI |

## Which ones matter most

Two are worse than 1538 itself, because they act rather than report:

* **`run_central_patch` WRITES into the root it picks.** From a worktree with an
  inherited value, `nros ws central-patch` with no `--nano-ros-path` writes the
  parent checkout's central patch file. That is a cross-tree write, and it is
  silent.
* **`rosidl-bindgen` emits path deps on that root.** Generated message crates in a
  worktree would name the PARENT's `packages/core/*`, so a worktree build compiles
  the parent's core crates — issue 1280's exact symptom, reached through codegen
  instead of a build script.

The two launch-resolver locators matter for issue 0409's reason: a resolver from a
different layer-2 checkout writes models that are MISSING DATA rather than
failing.

**All of this is read off the code, not reproduced.** In particular, whether
`nros sync` exports a corrected `NROS_REPO_DIR` to `rosidl-bindgen`'s process
before it runs would change that row's severity, and has not been checked.

## Why filed separately from 1538

1538's tool lives in `packages/tooling` and could reach `nros_build_paths::reroot_foreign`.
These live in the `packages/cli` sub-workspace (own lock), plus one core crate,
and they do not share one fix: two are drop-ins for 1510's resolver, one has a
`cwd` fallback whose replacement needs an error path, two have no workspace to
walk up from, and two sit in crates that cannot depend on `nros-cli-core`.
Folding that into a one-tool fix would have made it a seven-site change across
four crates without the per-site decisions.

## Acceptance

* Every `$NROS_REPO_DIR` reader in the CLI either goes through
  `nano_ros_root::resolve`, through a shared helper that applies the same rule
  where no workspace is in hand, or carries a comment saying why env-first is
  correct at that site.
* `run_central_patch` cannot write into a checkout other than the one it was
  run from without an explicit `--nano-ros-path`.
* A gate, so an eighth raw reader cannot land: the 0196 lesson is that this is
  now the third time the rule was fixed where the symptom was seen.
* A test in the nested worktree shape for at least the two acting sites.
