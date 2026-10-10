---
id: 1792
title: "`check-sizes-header-mirrors` compared 0 pairs on every machine: its globs named `packages/core/` after the crates moved to `packages/api/`, and its source was the leaf copy that issue 0978 had demoted to a fallback"
status: resolved
type: bug
severity: medium
area: [build, ci, testing]
related: [0268, 0978, 1739, 1783, 0196]
found: 2026-10-11
---

## Measured

`check-sizes-header-mirrors` exists to catch issue 0268's class: a per-build
sizes-header mirror (`<crate>/include/nros/nros_{,cpp_}config_generated.h`)
that differs from the header build.rs wrote. A stale mirror sizes the C
`_opaque` buffers from old data while Rust places the current object into
them, which is silent memory corruption.

The gate runs on the fast line before every push. On the main checkout on
2026-10-11 it printed:

```
[SKIPPED] sizes-header-mirrors: NOT VERIFIED — 0 mirror/source pair(s) across 0 build tree(s)
```

That checkout held 88 Ninja build dirs and 212 built mirrors. Of those, 12
were genuinely stale: the shared header had been rewritten by the tree's last
build (2026-08-28) and the mirror was left from 2026-08-26, which is the issue
1783 defect in an older tree. The gate reported none of them, on any machine,
for as long as the crates have lived under `packages/api/`.

## Cause

The gate found its pairs by guessing both halves.

- **Where a mirror is.** It used three globs under
  `nano_ros/packages/core/nros-*/include/nros/`. The crates moved to
  `packages/api/`, so the globs matched nothing. Issue 1739 had already turned
  "0 pairs" from `OK` into NOT VERIFIED, which was correct, but that made the
  blindness look like a missing precondition rather than a defect.
- **What it mirrors.** The gate compared each mirror against the leaf copy
  beside the crate's binary dir. Issue 0978 made that copy the fallback:
  `mirror-generated-header.sh` prefers the newest leaf-independent
  `cargo/*/<gen>/nros/<name>`, because a leaf copy can outlive the run that
  wrote it. So even with corrected globs, the gate would have checked a rule
  the build no longer follows, and it would have flagged correct mirrors.

This is issue 0196's shape. The gate's reach was narrower than its rule, and
here it narrowed to nothing.

## Fixed

`scripts/check-sizes-header-mirrors.py` replaces the shell gate and derives
both halves from the build itself:

- **Pairs** come from the commands each `build.ninja` actually contains:
  `mirror-generated-header.sh <leaf> <build> <gen> <name> <dest>`. Build dirs
  are found by a walk under `examples/`, `packages/` and `build/` that stops
  at nested repositories (`repo_walk.prune`) and skips `CMakeFiles`, `cargo`,
  `target*` and `generated`. No path or depth is written down. The walk takes
  0.6 s over the main checkout's 88 build dirs.
- **The source** comes from the mirror script itself, through the new
  `mirror-generated-header.sh --resolve`. The source-selection rule now lives
  in one place, and that script's self-test asserts that `--resolve` names the
  file the copy uses.
- **"Source gone" is not drift.** When the shared cargo copy has been deleted
  (a cleaned store), `--resolve` falls back to the leaf copy. If that leaf copy
  is older than the mirror, it cannot be what the mirror was copied from, so
  the pair is reported as "source gone" and not compared. Before this rule,
  the main checkout reported 159 pairs as drifted, every one of them a mirror
  newer than its leaf after its `build/` caches had been cleaned. A leaf copy
  newer than its mirror is still drift.
- **Makefile-generator build dirs** are counted and reported as not compared.
  Their commands live in `CMakeFiles/*/build.make`.
- **`--fix`** re-runs each drifted pair's own mirror command.
- **0 pairs is still NOT VERIFIED**, through the issue 1739 ledger.

The gate's self-test runs on the normal path. It builds a synthetic checkout
with the crate under `packages/api/` and a shared copy newer than a stale leaf
copy (0978's tree), and it covers drift, `--fix`, source gone versus a newer
leaf, and an unbuilt mirror. The gate has left `.config/gate-selftest-baseline.txt`.

## Evidence

On the main checkout, at the same moment:

| gate | result |
| --- | --- |
| old (`.sh`) | rc=0, `NOT VERIFIED — 0 mirror/source pair(s) across 0 build tree(s)` |
| new (`.py`) | rc=1, `12 stale sizes-header mirror(s)`; 65 pairs compared, 147 with source gone, 4 Makefile dirs |

A worktree built after issue 1783 compares 18 pairs with 0 drift.

Mutations of the new gate (rc; clean tree rc=0):

| mutation | rc |
| --- | --- |
| source = the leaf copy (the old rule) | 1 |
| no "source gone" rule | 1 |
| "source gone" swallows every leaf fallback | 1 |
| the command regex restricted to `packages/core/` (the old reach) | 1 |

## Not covered

- Makefile-generator build dirs (counted, not compared).
- The 12 stale trees in the main checkout were pre-1783 build residue and
  were deleted on 2026-10-11 at the maintainer's request; the gate then
  reads `OK — 29 pair(s) across 76 build dir(s)` there.
