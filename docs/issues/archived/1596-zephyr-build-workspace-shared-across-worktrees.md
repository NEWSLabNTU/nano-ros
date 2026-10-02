---
id: 1596
title: "Every worktree builds Zephyr images in ONE shared workspace, so parallel checkouts configure each other's build dirs and `zephyr-workspace-foreign-checkout` is red by construction"
status: resolved
type: bug
area: [build, zephyr, multi-session]
severity: medium
found: 2026-10-01
related: [issue-1280, issue-1387, issue-1593, issue-1638]
resolved_in: "branch fix/build-correctness-1593-1596-1599-1605"
---

## What happens

Zephyr west builds live in the shared workspace
`~/.nros/workspaces/zephyr/<ver>/build-*`, and the build-dir NAME is the
fixture's leaf name (`build-ws-rs-realtime-entry-zenoh`, …). It is the same
name in every checkout. Parallel agent sessions each work in their own
`.claude/worktrees/agent-*` checkout, so on 2026-09-29/30 three or more
worktrees configured the SAME build dirs in turn:

- `CMAKE_MAKE_PROGRAM`, `Zephyr-Kernel_SOURCE_DIR`, `_NANO_ROS_CODEGEN_TOOL`
  and 110+ other cache entries pointed at whichever worktree configured last
  (measured by `scripts/check-zephyr-workspace-foreign-checkout.py`).
- `just check zephyr-workspace-foreign-checkout` was red in EVERY agent's
  `just check fast`, and each one reported it as "environmental".

A gate that is always red carries no signal: a real cross-tree image, issue
1387's case, now looks exactly like the expected noise. Fixture results are
also unattributable, because a cell run from worktree A may execute an image
that worktree B configured, from B's sources. This is CLAUDE.md's "a test
result is only about the tree its FIXTURES were built from", made the
default.

Issue 1387's remedy, `retire-foreign-build-dirs`, deletes and rebuilds the
dirs. That is correct for a stray foreign dir. With parallel sessions it
destroys a dir another live session is using, and the dirs are re-crossed on
the next build anyway.

## Direction

The build dir's identity must include the checkout:

- either key the workspace on the checkout (e.g.
  `~/.nros/workspaces/zephyr/<ver>/<checkout-id>/build-*`, sharing the
  read-only west modules but not the build dirs);
- or put the build dirs under the checkout (`<repo>/build/zephyr/build-*`)
  with the west workspace still shared.

Both must keep the west-leaf routing, which is by build-dir NAME (issue 1016,
`check-west-leaf-vocabulary`), and the test-side resolvers
(`NROS_ZEPHYR_BUILD_ROOT`) on ONE derivation, never a second spelling.

## Acceptance

- Two worktrees building the same Zephyr fixture produce two build dirs;
  neither configures the other's.
- `zephyr-workspace-foreign-checkout` is green in a fresh worktree after a
  Zephyr build, and still red for a genuinely crossed dir (its negative
  control).
- The resolvers find each worktree's own images.

## Resolution

Took the second direction: **the build dirs belong to the checkout**, the west
workspace stays shared read-only source.

ONE derivation, `scripts/lib/zephyr-workspace.sh build-root` (shell function
`nros_zephyr_build_root`):

```
$NROS_ZEPHYR_BUILD_ROOT                                  as is, else
<nros_build_root>/zephyr-workspace-builds/<zephyr version>
```

`nros_build_root` is RFC-0070's root (`<checkout>/build`, `NROS_BUILD_ROOT`
relocates, an inherited `NROS_REPO_DIR` is re-rooted per issue 1280); the
version is a coordinate because 3.7 and 4.4 name leaves identically. The Rust
twin is `nros_tests::zephyr::zephyr_build_root()`; `fixtures::binaries`
delegates to it, and `build_root_derivation.sh` pins the two halves
(literal paths for default / 4.4 / `NROS_BUILD_ROOT` / override, plus the
twin's override, version and kind).

Readers moved onto it: `zephyr-ci.just` (fixture builder),
`zephyr-fixture-leaves.sh` (its own fourth workspace ladder, retired), the FVP
build/run recipes, `just zephyr clean`, the FVP and leaf-staleness tests, the
foreign-checkout gate (subject 2 and `--retire-foreign-build-dirs`),
`check-tier-priority-plan-image` (its "every image this tree built" sweep),
`check-action-client-arena-budget`, `check-orphan-generated-stamp`,
`gc-zephyr-builds`. Four `.config/zephyr-workspace-resolvers.txt` lines retired.
West-leaf routing (issue 1016) keys on the build-dir NAME and is unchanged
(`check-west-leaf-vocabulary`: 74 names, all modelled).

**Migration:** old dirs in the shared workspace are made UNUSED, never deleted,
so an in-flight build on another branch is untouched. The gate counts them as a
note (11 here), never a finding.

### Acceptance, measured

- **Two checkouts, two build dirs.** A second worktree of this branch
  (`git worktree add`, removed afterwards) derives
  `<wt2>/build/zephyr-workspace-builds/3.7`; this one derives
  `<wt>/build/zephyr-workspace-builds/3.7`. `build-c-talker-zenoh` and
  `build-rust-talker-zenoh` built into this worktree's root (cache:
  `APPLICATION_SOURCE_DIR`, `NROS_REPO_DIR`, `_NANO_ROS_CODEGEN_TOOL` all this
  worktree); neither name was created in the shared workspace.
- **Gate.** Before (origin/main gate, this host): RED, `228 cached value(s)
  across 10 of 10 build dir(s)` — other agents' dirs in the shared workspace.
  After: green in this worktree after the build, green in the fresh second
  worktree, and RED (`34 cached value(s) across 1 of 1`) when a CMakeCache from
  this worktree is planted in the second worktree's build root — the negative
  control on real data. The self-test also plants both directions (a crossed dir
  in the shared workspace is a note; the same crossing in the checkout's root
  fails).
- **Resolvers.** `boot_smoke::case_01_zenoh_rust_talker_boots` PASSED (52 s)
  against `build/zephyr-workspace-builds/3.7/build-rust-talker-zenoh`, with no
  such dir in the shared workspace. (It needed `ZEPHYR_NANO_ROS` set, because
  `zephyr_workspace_path()` — the "is Zephyr provisioned?" probe, not the image
  resolver — still does not know the store: that is open issue 1326.)

Found on the way, filed as **issue 1638**: the first build's cache named the
MAIN checkout's `third-party/ninja/ninja` as `CMAKE_MAKE_PROGRAM` (an inherited
PATH entry; `nros sdk-path ninja` names an unprovisioned dir), and the gate
correctly reported it; with those PATH entries removed it read `/usr/bin/ninja`
and the gate was green.

Not measured: the 4.4 line, the FVP recipes (license-gated), CI runners.
