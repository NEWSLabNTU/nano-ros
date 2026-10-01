---
id: 1596
title: "Every worktree builds Zephyr images in ONE shared workspace, so parallel checkouts configure each other's build dirs and `zephyr-workspace-foreign-checkout` is red by construction"
status: open
type: bug
area: [build, zephyr, multi-session]
severity: medium
found: 2026-10-01
related: [issue-1280, issue-1387, issue-1593]
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
