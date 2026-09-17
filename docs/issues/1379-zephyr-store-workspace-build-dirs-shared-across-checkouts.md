---
id: 1379
title: "The store's Zephyr workspace holds ONE `build-<leaf>` dir per example,
  shared by every checkout, so a west build in one worktree compiles another
  clone's absolute source paths"
status: open
type: bug
area: [build, zephyr, testing]
severity: high
found: 2026-09-17
related: [issue-1248, issue-1280, issue-0925, issue-1243]
---

## What happens

`just zephyr build-one rust/listener zenoh` in a fresh agent worktree built
into `/home/aeon/.nros/workspaces/zephyr/3.7/build-rust-listener-zenoh`, and
the C half of that build compiled sources from a DIFFERENT checkout:

```
[1271/1315] Building C object modules/nros/CMakeFiles/nros.dir/
  home/aeon/repos/simple-autoware-safety-island/third-party/nano-ros/
  packages/rmw/zenoh/zpico-sys/c/zpico/zpico.c.obj
```

The Rust half correctly used the invoking checkout
(`/home/aeon/nros-agent-worktrees/392-b/...`), so the image being linked was
half one tree and half another. Nothing warned.

## Why it happens

`scripts/lib/zephyr-workspace.sh`'s resolution ladder ends at
`$NROS_STORE/workspaces/zephyr/<version>` — the STORE, which is shared by every
checkout on the host — and the build directory inside it is named from the LEAF
only (`build-<example>-<rmw>`). Two checkouts building the same example
therefore target the same cmake build dir. cmake caches absolute source paths
at configure time and does not reconfigure merely because a different tree
invoked it, so the second checkout inherits the first one's paths for every
target whose cache entry did not change.

This is the failure mode CLAUDE.md's "BUILD WHERE YOU RUN" rule exists to
prevent, arrived at from the other direction: `ros2-box-sync.sh` was retired
because provisioned source and build output share directories, and this is the
same sharing one layer up. The store arm is deliberately LAST in the ladder
(phase-440 W1 / RFC-0095 D4) so that nothing MOVED when it landed — but for a
checkout with no in-tree workspace, last is the only arm that resolves.

## Why it is high severity

It is silent and it is cross-tree. A worktree's build can be linked against
another clone's sources — including sources from a branch that clone is
mid-edit on — and the resulting image passes or fails for reasons belonging to
a tree nobody was looking at. With several agent worktrees on one box (which is
how this repo is worked) two sessions can also fight over the same build dir.

## What was not measured

Whether the Rust/C split above is the general shape or just what this build
happened to reach first, and whether a `cmake <build-dir>` reconfigure repairs
it (`just reconfigure-stale` is the tool, and this is exactly a
generated-build-file problem, so it should be tried before anyone reaches for
`rm -rf` — CLAUDE.md's rule applies).

## Direction

The build dir has to carry the checkout's identity, not just the leaf's — e.g.
`build-<leaf>-<hash of the resolved repo root>` — or the store workspace has to
stop being a build location and only be a SOURCE location, with builds landing
under the invoking checkout. The second is closer to what "build where you run"
means; the first is the smaller change.

Found while making the phase-392 amendment B measurement
([`docs/roadmap/phase-392-static-memory-space-campaign.md`](../roadmap/phase-392-static-memory-space-campaign.md)),
which had to abandon a Zephyr measurement image because of it.
