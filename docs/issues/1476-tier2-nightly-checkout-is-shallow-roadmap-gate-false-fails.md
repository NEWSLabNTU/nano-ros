---
id: 1476
title: "`check-roadmap-commit-refs` runs in a shallow checkout on the tier-2 nightly
  and reports every citation unreachable — 3 of 3 FALSE, and a documentation gate is
  what now stops tier 2 from reaching a cell"
status: open
type: bug
area: ci, docs, testing
severity: high
found: 2026-09-24
related: [1158, 1345]
---

## What happens

`nightly` run **35958890602** (schedule, 05:12), job **107502987427**
(`tier 2 nightly (pairwise cover)`), step **`just build tier2-nightly`**:

```
===== FAIL (roadmap-commit-refs, rc=1, 1139ms) =====
check-roadmap-commit-refs selftest: OK (7 pattern case(s), 5 classify case(s))
check-roadmap-commit-refs FAILED: 3 of 3 pull-request citation(s) name a commit HEAD cannot reach.
  docs/roadmap/phase-459-…:228: PR #1219 (6918743af) -- not an ancestor of HEAD
  docs/roadmap/phase-461-…:314: PR #1223 (e0ef2d5a2) -- not an ancestor of HEAD
  docs/roadmap/phase-462-…:146: PR #1170 (34ef4405f) -- not an ancestor of HEAD
error: recipe `roadmap-commit-refs` failed on line 287 with exit code 1
error: recipe `fast` failed on line 138 with exit code 1
error: recipe `build` failed with exit code 1
```

**All three ARE ancestors** of the exact commit that run checked out. Verified
in a full clone against `f74ebdade`, the run's own head:

```
6918743af IS an ancestor of f74ebdade
e0ef2d5a2 IS an ancestor of f74ebdade
34ef4405f IS an ancestor of f74ebdade
```

So the verdict is 3 of 3 false, and the run built nothing.

## Why the gate believes otherwise

`.github/workflows/nightly.yml:1119` — the `tier 2 nightly` job's
`actions/checkout@v4` carries **no `fetch-depth`**, so it is shallow. Two other
jobs in the same workflow set `fetch-depth: 0` explicitly, with a comment
saying why.

The gate resolves each commit's SUBJECT and prints it, so the objects are
present; what is missing is the ancestry chain, which is what
`merge-base --is-ancestor` walks. That is why the message reads as a confident
claim about history rather than as a missing object.

This is the same class as `gate.yml`'s own documented hazard — its comments at
lines 194 and 518 record that `actions/checkout` is shallow and that a check
built on `origin/main` silently answered the wrong question for as long as it
existed. This one fails closed instead of open, which is better, but the
verdict is still not about the documents.

## Why it matters

The gate runs inside `check fast`, which runs inside `just build
tier2-nightly`. So a **documentation** gate now stops the tier-2 nightly before
it builds a single fixture — the lane whose long-standing complaint (issue
1158) is that it never reaches a cell. Every citation it names is correct, and
the lane it blocks is the one with the least signal capacity to spare.

## What this is NOT

- **Not stale citations.** The three commits are on `main` and were on `main`
  when the run started.
- **Not issue 1158.** That is tier 2 not reaching cells for provisioning and
  build reasons. This is a doc gate refusing before the build begins, and it
  post-dates 1158 by weeks.
- **Not the same defect as the pre-merge report on PR #1230.** There the gate
  could not see the commits at all (`not a commit in this repository at all`,
  12 of 12). It can see them now; it cannot see the history between them.

## What would close it

Two defensible fixes, and this repository has already chosen between them once:

1. **Give the job the history** — `fetch-depth: 0` on the tier-2 nightly's
   checkout, matching the two jobs in the same workflow that already do it.
   Simple, and pays a clone cost on a lane that then builds for an hour.
2. **Make the gate say it cannot verify** — the three-outcome shape
   `check-submodule-pins` already uses for exactly this situation (FAIL when
   ancestry is MEASURED and wrong, NOT VERIFIED when the object store cannot
   answer, OK otherwise), reported through the `nros_check_skip` ledger. A gate
   that fails closed on an unanswerable question is a gate that blocks lanes
   it was never meant to police.

Whichever lands, the general rule is worth a gate of its own: a check that
walks history must either be given history or declare that it was not.
