---
id: 647
title: "The identity budget's tree-wide ceiling counted rlibs from earlier builds, so a correct incremental tree fails it — with `rm -rf` as the only remedy"
status: resolved
type: bug
severity: medium
area: build/ci
related: [issue-0499, issue-0513, issue-0616, phase-340]
---

## Symptom

`just ci` fails at `check-artifact-identity-budget`:

```
artifact-identity budget: FAIL
  counted ALL 277 rlib(s): this build rebuilt 37 of them and none for nros_core,
  so the started_at window says nothing about it (issue 0513).
  crates over the tree-wide ceiling of 5 identities in …/mixed/build-workspace-fixtures:
  nros: 6
```

The prescribed remedy is to delete the tree (2.5 GiB) and rebuild (~7 min).
Doing so reports `worst crate 5/5` — exactly ON the ceiling. Hit twice in one
session on trees with no defect.

## Cause

The gate answers two different questions from one artifact list:

1. the **named budget** — how many identities of `nros_core` does this tree hold;
2. the **tree-wide ceiling** — does any crate exceed 5 identities / 5 copies.

Issue 0499 gave it an era filter (`started_at` from the fixture stamp) so it
counts what THIS build wrote. Issue 0513 then added a widening: when the build
legitimately rebuilt nothing for `nros_core`, the window cannot answer question
1, so the gate falls back to the whole tree and labels the reading unfiltered.

That widening replaced the artifact list **globally**, so question 2 was
answered from the accumulated tree too. Cargo never collects the rlibs a
previous build left, so any incremental rebuild that changes a fingerprint
leaves the old identity on disk beside the new one — and a clean build of this
workspace already sits at exactly 5, the ceiling. One incremental rebuild is
therefore enough to fail the gate, and the only way back is deleting the tree,
which measures green because it erases the history it was counting.

Widening is right for question 1 (it can only over-count, and an over-budget
crate still gets reported). It is wrong for question 2, which is about what a
build PRODUCED.

## Fix

Keep both views. The widened list still answers the named budget; the tree-wide
ceiling and copies axes read `triples_era` — the rlibs written since
`started_at` — whenever a window exists, and say how many crates they therefore
do not judge:

```
tree-wide axes read the 5 rlib(s) THIS build wrote (1 crate(s));
1 crate(s) it did not compile are not judged here (issue 0647).
```

This cannot create a false green. A build that really compiles six units of a
crate writes all six inside the window; what the era view drops is crates this
build never compiled, which is exactly what it has nothing to say about — and a
change that duplicates a unit necessarily compiles it, so the build that
introduces the regression is the build that reports it.

There were TWO widening sites (one before the artifact list is parsed into
triples, one after, for a different sub-case). Fixing only the second left the
reported symptom intact — the real-tree reproduction is what caught it.

## Verification

Synthetic tree (`NROS_IDENTITY_BUDGET_TREE` + `NROS_FIXTURE_STAMP`), four
directions:

| scenario | want | got |
| --- | --- | --- |
| 5 current + 1 stale identity, budget crate rebuilt | pass | rc=0 |
| 6 current identities, budget crate rebuilt | fail | rc=1 |
| 5 current + 1 stale, budget crate NOT rebuilt (the reported case) | pass | rc=0 |
| 6 current, budget crate NOT rebuilt | fail | rc=1 |

And on the real tree, planting a sixth `libtoml-*.rlib` in the workspace that
sits at the ceiling: current mtime → rc=1; the same file back-dated before the
build → rc=0.

## Left alone

The ceiling is 5 and a clean build produces exactly 5, so this axis still has no
headroom — a genuine sixth unit fails on its first appearance, which is the
intent, but it also means the number cannot absorb any legitimate growth without
a deliberate re-audit. That is phase-340 item 8's business (lower the budgets per
axis), not this fix's.
