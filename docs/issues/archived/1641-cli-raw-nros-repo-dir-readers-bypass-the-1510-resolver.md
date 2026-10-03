---
id: 1641
title: "Seven CLI-side readers take `$NROS_REPO_DIR` raw and bypass issue 1510's
  resolver, so from a linked worktree they act on the PARENT checkout — one
  writes a file into it, and one emits path deps that compile its crates"
status: resolved
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

## Fix — 2026-10-03

Every reader now hands the value to something that puts the caller's OWN
checkout first, and says which at the site. Per site, because they did not
share one fix:

| site | now | why not the 1510 resolver, where it is not |
| --- | --- | --- |
| `cmd/build.rs` | `nano_ros_root::resolve` | — a read, and a drop-in |
| `cmd/leaf_system.rs` | `nano_ros_root::resolve` | — a read, and a drop-in |
| `cmd/ws.rs` `run_central_patch` | `central_patch_root`: explicit > the checkout CONTAINING cwd > env > **error** | it WRITES. `resolve`'s last rung is the installed toolchain's own `share/nano-ros`, and writing there is worse than the bug. The old last resort, "the current directory itself", wrote a patch file into whatever directory the command ran in; it now refuses and names `--nano-ros-path` |
| `cmd/ws.rs` `resolver_from` | the RUNNING CLI's own checkout moved ABOVE `$NROS_REPO_DIR` | it locates a binary, not an SDK. The order contradicted its own comment ("each worktree carries its own tools, with no cross-tree skew") |
| `orchestration/sdk_store.rs` | `nano_ros_root::resolve(None, cwd)` | — a read; matters exactly when a worktree tests a toolchain bump |
| `rosidl-bindgen` `nros_dep_line` | `checkout_for_output`: the checkout CONTAINING the generated crate, else env | it is below `nros-cli-core`; `nros-launcher`'s `find_monorepo_root` is reachable and is the CLI's one marker walk |
| `nros-orchestration-ir` `launch_resolver_bin` | new `launch_resolver_bin_near(near)`, through `nros_build_paths::reroot_foreign`, called with the launch file | a core crate, so it takes the canonical walker rather than a fourth copy of the marker that `check-inherited-checkout-paths` would not know about. A rerooted checkout with no built resolver falls through to the store rather than borrowing the parent's |

The "severity unknown" row is resolved by the fix rather than by measurement:
whatever `nros sync` exports to `rosidl-bindgen`, the generated crate's own
checkout now wins inside a checkout, so the answer no longer depends on it.

### Lockfiles

`rosidl-bindgen` gains `nros-launcher` and `nros-orchestration-ir` gains
`nros-build-paths`. Both were already in every affected graph, so the 16 locks
moved by exactly 16 × `+ "nros-build-paths",` and 1 × `+ "nros-launcher",` —
insertions only, no package added or removed, no version moved. Recorded with
`just lock-update "" "" <dir>` per lock.

### Tests, in the nested shape

Six new tests, built on disk in the shape agent worktrees actually have
(`<main>/.claude/worktrees/<id>`, so the parent's root is a strict prefix of the
worktree's — issue 1391), plus the out-of-tree case each ladder must keep:

* `central_patch_from_a_worktree_does_not_write_into_the_parent`
* `central_patch_outside_every_checkout_takes_the_named_one_or_refuses`
* `a_worktree_cli_does_not_borrow_the_parents_resolver`
* `an_out_of_tree_cli_still_takes_the_named_checkouts_resolver`
* `a_generated_crate_in_a_worktree_depends_on_the_worktree`
* `a_copy_out_project_takes_the_named_sdk_or_none`

**Mutation-checked:** reverting each of the three new ladders to env-first fails
exactly its own nested-worktree test.

### The gate — the class, after four sightings

`check-repo-dir-readers` (fast line): every Rust read of `$NROS_REPO_DIR`
outside test code must carry a `repo-dir-env-ok: <reason>` comment within three
lines, naming where the rule is applied. A reason at the site rather than an
allowlist, because an allowlist does not move when the code does.

Verified against the **pre-fix tree**, not only its selftest: run over
`origin/main`'s sources it flags all seven readers this issue listed, plus the
two that were already correct (1510's `nano_ros_root`, 1538's tool) and simply
stated no reason. After the fix: 6 reads, each naming its rule.
