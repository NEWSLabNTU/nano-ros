---
id: 957
title: "`just format` fails whole-tree — a workspace leaf is in neither the root members nor the exclude list"
status: open
area: build
severity: low
found: 2026-08-31
related: [phase-383, phase-411]
---

# The documented pre-change practice does not run

CLAUDE.md says "**`just format` before broad changes**". It cannot complete:

```
$ just format
parallel: This job failed:
cd examples/workspaces/bridge-xrce/src/talker_pkg && cargo +nightly-2026-04-11 fmt
error: recipe `format` failed with exit code 1
```

The cause:

```
`cargo metadata` exited with an error: current package believes it's in a
workspace when it's not:
current:   examples/workspaces/bridge-xrce/src/talker_pkg/Cargo.toml
workspace: <repo-root>/Cargo.toml
```

The leaf is in neither the root `workspace.members` nor `workspace.exclude`, and
carries no `[workspace]` table of its own — so cargo resolves it into the root
workspace, which does not list it.

This is the class CLAUDE.md already records for west-built Zephyr entry leaves:
they "need BOTH the nested workspace `exclude` AND a repo-root `Cargo.toml`
exclude". `bridge-xrce` has neither half.

## Pre-existing, and measured as such

* the leaf's last commit is `e7c47132e feat(phase-383 W10.a/W10.c)` — the bridge
  migration, not any phase-405/407 work;
* `git show origin/main:Cargo.toml | grep -c bridge-xrce` -> **0**, so `main` is
  identical;
* `cargo +nightly fmt --check` over the ROOT workspace is clean (rc=0), so the
  failure is the leaf's membership, not formatting drift.

## Why it is only `low`

`just format` fails LOUDLY, at the offending leaf, naming the fix cargo itself
suggests. Nothing is silently unformatted, and per-crate `cargo +nightly fmt`
works everywhere else. The cost is that the whole-tree practice has to be
skipped or worked around, which is how a documented habit quietly stops being
one.

## Work

1. Decide which half applies — an `[workspace]` table in the leaf, a root
   `exclude` entry, or both (the west-leaf precedent takes both).
2. Sweep for siblings: any `examples/workspaces/*/src/*/Cargo.toml` that is in
   neither list. Fixing only the reported leaf is the failure mode CLAUDE.md
   files under "fix the CLASS, not the reported site".
3. Gate it — membership is statically checkable from the root manifest plus the
   leaf list, and this is at least the third time the two-excludes rule has been
   half-applied.
