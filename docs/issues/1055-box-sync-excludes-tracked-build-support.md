---
id: 1055
title: "`ros2-box-sync.sh` excluded `packages/cli/build-support/` as build output,
  so the ROS box could not compile `nros` — the fourth time that rule has eaten
  tracked source"
status: resolved
type: bug
area: [ci, build, process]
related: [0400, 0401, 0759]
---

## What

Building anything in the ROS distrobox failed at the first step:

```
error: couldn't read `nros-cli-core/../build-support/submodule_watch.rs`:
  No such file or directory (os error 2)
error: could not compile `nros-cli-core` (build script)
```

`scripts/dev/ros2-box-sync.sh` mirrors the tree into the box and excludes build
output by name pattern. One of those is `--exclude 'build-*/'`, and
`packages/cli/build-support/` is **tracked source**. The mirror simply did not
contain it, so the box could not build `nros`, so nothing else could run.

## The class, and why the two previous fixes did not close it

The sync script's own header records the first three incidents:

| pattern | ate | discovered as |
| --- | --- | --- |
| `build` | `scripts/build/` | `check-board-manifest-drift` failing on a missing `cargo.sh` |
| `build-*` | `scripts/build/build-root.sh` | "No such file or directory" mid fixture build |
| — | 21 tracked files with a `build-*` basename | silently absent |
| **`build-*/`** | **`packages/cli/build-support/`** | **the box cannot compile `nros`** |

Fix 1 anchored the pattern. Fix 2 made every pattern end in `/` so it matches
DIRECTORIES ONLY — which is correct and is what rescued those 21 files. Neither
addressed the remaining case: **a tracked directory whose name begins with
`build-` still matches a directory-only pattern.**

Every one of the four was found from inside the box, as a build error naming
something other than the mirror. That is the expensive part: the failure is
displaced from its cause by a whole environment.

## Fixed

`/packages/cli/build-support/***` is re-included AHEAD of the exclusion, the
technique the script already uses for `/zephyr-workspace/**/build/***` (rsync
takes the first matching rule). There is exactly one such directory today:

```bash
git ls-files | grep -oE '(^|/)build-[^/]+/' | sort -u
```

## Gated

`check-box-sync-covers-tracked-source` (fast line, offline) parses the sync
script's `--include` / `--exclude` rules in order and asks whether any tracked
path would be dropped. Removing the re-include reproduces the bug exactly:

```
check-box-sync-covers-tracked-source: 1 TRACKED path(s) would not reach the box mirror
  packages/cli/build-support/submodule_watch.rs
      excluded by  --exclude 'build-*/'
```

**The checker's own first version was wrong in an instructive way.** It stripped
the trailing `/` from patterns and so reported `build-all.mk` and 30 book pages
as lost — the very files fix 2 had rescued. Directory-only semantics are the
load-bearing part of that fix, and a gate that ignores them cries wolf about it.
Modelled explicitly now.

## Left as a finding, not fixed

Ten tracked files live under `tmp/`, which `.gitignore:50` excludes — the
`collapse-*-case.sh` repro scripts and two `migrate-*.py`. Each needed
`git add -f`, so each was deliberate; none is a build input, and the box does
not need them. The gate allowlists the prefix and says so in its output rather
than skipping silently, because "tracked under a gitignored path" is worth
someone's attention even when it is harmless.

## The rule this sits under

CLAUDE.md: **box in play ⇒ every job in the box, on its own tree.** The two-tree
split exists because host and box artifacts are glibc- and toolchain-specific;
this bug is the mirror half of that contract failing, and it fails silently —
the box gets a tree that looks complete.
