---
id: 894
title: "`just format` fails repo-wide: phase-383 W10.a deleted the `launch`
  workspace root and its two leaves now resolve against the REPO root"
status: resolved
type: bug
area: examples, build
related: [phase-383]
---

## Problem

`just format` fails for everyone:

```
cd examples/workspaces/launch/src/listener_pkg && cargo +nightly-2026-04-11 fmt
`cargo metadata` exited with an error: current package believes it's in a workspace when it's not:
current:   examples/workspaces/launch/src/listener_pkg/Cargo.toml
workspace: /mnt/wd/data/projects/nano-ros/Cargo.toml
```

`8b15506b2` (phase-383 W10.a, "migrate `sizing` and `launch`") deleted
`examples/workspaces/launch/Cargo.toml`. That file was the workspace root its
two leaves resolved against; without it, cargo's walk-up reaches the REPO root,
which does not list them as members and does not exclude them either.

## Why only these two

Checked rather than assumed. Of the 25 leaves under `examples/workspaces/*/src/`
that carry neither an `[workspace]` table nor a repo-root exclude, 23 still have
a surviving parent root:

```
bridge-cyclonedds: root EXISTS      launch:         root GONE
bridge-xrce:       root EXISTS      realtime-rust:  root EXISTS
features:          root EXISTS      rust:           root EXISTS
```

So the migration's other workspaces kept their roots and `launch` is the
anomaly, not the pattern. Two leaves are affected: `listener_pkg` and
`talker_pkg`.

## Fix

Both added to the repo root's `exclude`, with the reason recorded there. That
matches how the other standalone example leaves are handled — the root already
excludes `examples/native/rust/*`, the px4 companions and the qemu leaves for
exactly this reason.

The alternative, an empty `[workspace]` table in each leaf manifest, was not
taken: these are `nros build` workspaces whose generated roots live under
`build/<coord>/`, and the repo-root exclude is where every sibling records the
same fact. One place to look beats two.

This is the class CLAUDE.md already names for west leaves — "BOTH the nested
workspace exclude AND a repo-root `Cargo.toml` exclude" — reached from the other
direction: a root going away rather than a leaf arriving.

## Verified

`cargo +nightly fmt` in `listener_pkg` exits 0 where it previously failed at
`cargo metadata`. Found while running `just format` for unrelated work; the
failure is not caused by that work and reproduces on a clean tree.
