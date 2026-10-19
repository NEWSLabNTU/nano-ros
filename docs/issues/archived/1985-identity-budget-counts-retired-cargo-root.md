---
id: 1985
title: "`artifact-identity-budget` counts a cargo root the image no longer builds, so a persistent fixture tree fails it after phase-506 W4a"
status: resolved
type: bug
area: [ci, build]
severity: medium
found: 2026-10-19
related: [issue-0499, issue-0513, issue-0647, issue-1922, phase-340, phase-505, phase-506]
resolved_in: 2026-10-20
---

**Home: [phase-505](../../roadmap/phase-505-ci-lanes-reach-and-infra-robustness.md)** — listed in that phase's "Issues homed here".

## What failed

The scheduled run-matrix run 39210994277 (2026-10-19, head `c75d1265eb`) went
red in its tier 1 job before building anything. `just build tier1` runs
`check fast` first, and 1 of its 461 gates failed:

```
artifact-identity budget: FAIL
  counted ALL 512 rlib(s): this build rebuilt 58 of them and none for nros_core, …
  nros_core has 6 distinct -C metadata identities in examples/workspaces/mixed/build/posix-zenoh-linux/cmake
  (budget 4, recorded 2026-08-07 by phase-340 W4).
    c980a9b41ad4c8bf  …/cargo/nros-c-image_593ef/nros-relwithdebinfo/deps/…
    68f62a5f986cb0a5  …/cargo/nros-c-image_593ef/x86_64-unknown-linux-gnu/…
    07bdc43d225717f0  …/cargo/nros-fixture-tree_15c46/nros-relwithdebinfo/deps/…
    815df63bb07300d4  …/cargo/nros-fixture-tree_15c46/x86_64-unknown-linux-gnu/…
    02d62c104403d188  …/cargo/nros_ws_runtime_fee5a/nros-relwithdebinfo/deps/…
    49df2e5cd1b16d7d  …/cargo/nros_ws_runtime_fee5a/x86_64-unknown-linux-gnu/…
```

## Cause

The gate counted history.

phase-506 W4a (`d0d295647e`, 2026-10-18) moved the C/C++ API crates
from the repo-root cargo workspace into a generated image crate (RFC-0071 D9).
Corrosion gives each imported root its own `cargo/<root>_<hash>/`, so a
configure since then builds `nros-c-image_*` / `nros-cpp-image_*` and
`nros_ws_runtime_*`, and no longer builds the repo-root one, which is
`nros-fixture-tree_*` in the fixture tree (the root is named after the checkout
directory). The persistent tree kept that root from its earlier builds.

Measured: a fresh `workspace-fixtures-build.sh linux mixed` of `origin/main`
writes exactly three roots and reads **nros_core 4/4** (the image-crate pair,
shared by the C and C++ image roots, and the `nros_ws_runtime` pair).

Neither existing defence covers a retired ROOT:

- The `started_at` window (issue 0499) cannot help, because `check fast` runs before the
  lane's build. That build had not rebuilt `nros_core`, so the gate widened to
  the whole tree (issue 0513) and said the count "MAY include earlier builds".
  Then it failed anyway.
- `prune-superseded-artifacts.py`, the remedy the gate prints, keeps the
  newest file per DIRECTORY. A retired root is a directory of its own, so its
  one identity per slot is never pruned.

## Resolution

The gate asks the configure which roots it builds. Every live root is a
`--target-dir=<tree>/cargo/<root>` in a generated `build.ninja` / `build.make`
of the image tree. The gate leaves out rlibs under a `cargo/<root>/` that none
of them names, and the reading says so:

```
artifact-identity budget OK (…/posix-zenoh-linux/cmake): nros_core 4/4 identities; …
  left out 2 rlib(s) under cargo root(s) no build file of this configure names (issue 1985): prm_15c46
```

A tree with no generated build files (a synthetic tree, a half-configured dir)
gets no filter, because with no evidence nothing is left out. The name match is
anchored, so a live `live_1` does not also keep a retired `live_12`.

Reproduced on a fresh tree with the CI run's two extra hashes planted in a fake
retired root: OK 4/4 with the note. With the filter mutated out: `FAIL … nros_core
has 6 distinct -C metadata identities`, the CI failure.

Self-test (`--self-test`, on the fast line through the recipe) gains two
cases:

- (i) A retired root, whose name has the live root's name as a prefix, is left
  out and named.
- (j) The negative control: once a build file names that root it is counted
  again, and the tree FAILS at 6.

Self-test: 10 cases OK.

**Not done:** the runner tree's retired root is still on disk. The gate stops
counting it, and nothing deletes it.
