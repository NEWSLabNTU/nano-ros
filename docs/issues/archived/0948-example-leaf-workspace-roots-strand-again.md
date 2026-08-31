---
id: 948
title: "`just format` fails repo-wide again: the two bridge workspace roots were
  deleted and their `talker_pkg` leaves resolve against the REPO root"
status: resolved
type: bug
area: examples, build
related: [0894, phase-383, phase-406]
---

## Problem

`just format` fails for everyone, in two leaves at once:

```
cd examples/workspaces/bridge-xrce/src/talker_pkg && cargo +nightly-2026-04-11 fmt
`cargo metadata` exited with an error: current package believes it's in a workspace when it's not:
current:   examples/workspaces/bridge-xrce/src/talker_pkg/Cargo.toml
workspace: /mnt/wd/data/projects/nano-ros/Cargo.toml
```

`examples/workspaces/bridge-cyclonedds/` and `examples/workspaces/bridge-xrce/`
no longer carry a `Cargo.toml`. Each still has `src/talker_pkg/`, a real tracked
`[package]`, so cargo's walk-up from that leaf reaches the repo root — which
lists it in neither `members` nor `exclude`.

## This is 0894, a second time

Issue 0894 was the same failure in `examples/workspaces/launch`, whose root
phase-383 W10.a deleted. Its fix added the two stranded leaves to the repo
root's `exclude`, and it explicitly checked the siblings, recording:

```
bridge-cyclonedds: root EXISTS      launch:         root GONE
bridge-xrce:       root EXISTS      realtime-rust:  root EXISTS
```

That was true when written, and it concluded "`launch` is the anomaly, not the
pattern". The bridge roots were deleted after that survey, and the anomaly
became the pattern.

The recurring shape is worth naming precisely, because it is not "someone forgot
an exclude": **deleting a workspace root strands leaves in a directory the
deleter is not editing**, and nothing connected the two. A survey answers the
question on the day it is run.

## Fix

Two parts, per the CLAUDE.md rule that a class gets a gate and not just a patch:

1. Both leaves added to the repo root `exclude`, beside the `launch` pair and
   for the same recorded reason.
2. **`check::example-leaf-workspace-roots`** (`scripts/check-example-leaf-
   workspace-roots.py`, fast line) — every tracked `[package]` under
   `examples/` must resolve to some workspace root; if the walk-up lands on the
   repo root, the leaf must be named there. It self-tests its `members` /
   `exclude` array parser and its walk-up on every run.

Scope is tracked manifests only (`git ls-files`): untracked ones are build
output, and `_deps/corrosion-src/test/**` alone contributes ~90 package
manifests no cargo command in this repo resolves.

## Verified

- The gate reproduces the defect before the fix (names exactly the two leaves)
  and passes after it: `110 tracked example manifest(s), none stranded`.
- Swept all 110 rather than patching the two that happened to fail: those two
  are the complete set.
- `just format` completes.
