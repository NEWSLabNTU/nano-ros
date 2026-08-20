---
id: 718
title: "A build directory written into an example leaf was not ignored, so a fixture build left untracked object files in `git status`"
status: resolved
type: bug
severity: medium
area: build
related: [issue-0692, rfc-0026]
resolved_in: "2026-08-20 — `check-example-leaf-build-dirs`"
---

# 0718 — a leaf's build directory has to be named in that leaf's own `.gitignore`

An example leaf is a standalone copy-out project (RFC-0026), so its build output
lands *inside* the tree. There is no repo-root pattern for `examples/**/build*`
— `build` is a legitimate tracked name elsewhere, and a blanket rule would hide
real files — so each leaf names its own build directories.

## What happened

`just threadx_riscv64 build-fixtures` builds the six `examples/qemu-riscv64-threadx/rust/`
leaves for **both** RMWs:

```
build_threadx_cmake_rmw "examples/qemu-riscv64-threadx/rust/$_rc" cyclonedds build-cyclonedds
build_threadx_cmake_rmw "examples/qemu-riscv64-threadx/rust/$_rc" zenoh      build-zenoh
```

Their `.gitignore` files listed `/build-cyclonedds/` and not `/build-zenoh/`. So
a fixture build left six untracked directories of object files in `git status`:

```
?? examples/qemu-riscv64-threadx/rust/action-client/build-zenoh/
?? examples/qemu-riscv64-threadx/rust/action-server/build-zenoh/
?? examples/qemu-riscv64-threadx/rust/listener/build-zenoh/
?? examples/qemu-riscv64-threadx/rust/service-client/build-zenoh/
?? examples/qemu-riscv64-threadx/rust/service-server/build-zenoh/
?? examples/qemu-riscv64-threadx/rust/talker/build-zenoh/
```

That is precisely the state in which a blanket `git add -A` commits build output,
which is why CLAUDE.md bans the blanket add — but the ban is a rule about the
person staging, and this is the condition that makes the rule load-bearing.

## Why no one saw it

The asymmetry is invisible from any one leaf. The `c/` and `cpp/` leaves of the
same platform list **both** directories, and every rust leaf lists a
`build-cyclonedds/`, so each file reads as complete on its own. The defect only
appears when the two RMW arms of one leaf are compared against each other:

```
$ for gi in $(find examples -mindepth 3 -name .gitignore); do
      d=$(dirname "$gi")
      cy=$(grep -c '^/build-cyclonedds/$' "$gi"); ze=$(grep -c '^/build-zenoh/$' "$gi")
      [ "$cy" != "$ze" ] && echo "$d cyclone=$cy zenoh=$ze"
  done
```

That sweep also names `examples/native/rust/*` and two
`examples/templates/multi-package-workspace/src/*` packages, but those are not
defects: nothing gives them a `build-zenoh/`, and the directories they do get
(`/build/`) are already ignored. Asymmetry is the smell; the rule is about
directories a recipe actually writes.

The pre-existing `check-example-leaf-target-dirs` gate does not cover this. It
is about cargo's default `target/`, a different class with a different fix (pass
`--target-dir`), and it says nothing about cmake build directories.

## Fix

`/build-zenoh/` added to all six rust leaves — the whole class, not the leaf the
symptom was noticed in.

Gated by `check-example-leaf-build-dirs` (`scripts/check-example-leaf-build-dirs.py`,
on the `just check` fast line): no directory under `examples/` whose basename
begins with `build` may be untracked.

The gate asks `git status --porcelain` rather than walking the filesystem, so it
carries no path logic of its own and is correct for every platform's naming. The
cost is that it is a **post-build** check — on a tree that has never been built
there is nothing to see. That is the honest shape here: which build directories
a leaf gets is decided by shell variables inside the `just` recipes
(`build_threadx_cmake_rmw`'s third argument is a positional parameter), so a
static parse would have to guess, and a guess is either noise or a false green.

## How it surfaced

While confirming issue 0692's resolution: the threadx-riscv64 family was rebuilt
from deleted build directories, and the six untracked leaves appeared in the
`git status` taken afterwards.
