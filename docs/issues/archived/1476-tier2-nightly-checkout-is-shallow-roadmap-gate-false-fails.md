---
id: 1476
title: "`check-roadmap-commit-refs` runs in a shallow checkout on the tier-2 nightly
  and reports every citation unreachable — 3 of 3 FALSE, and a documentation gate is
  what now stops tier 2 from reaching a cell"
status: resolved
type: bug
area: ci, docs, testing
severity: high
found: 2026-09-24
related: [1043, 1158, 1345, 0650, 0196]
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

## Resolution

Option 2, plus the gate the last paragraph asked for. What the fix had to add
beyond the plan above is the reason the existing guard did not save the lane.

### Why the shallow guard did not fire

The gate already had one. `is_shallow()` and a `classify()` that returns
`unknowable` instead of failing were written for the PR #1230 report, and its
module header says in as many words that a previous version "said the exact
opposite of the truth" in a shallow clone. It still failed here, because it
guarded only ONE of the gate's two negatives:

```python
if git(root, "cat-file", "-e", f"{sha}^{{commit}}").returncode != 0:
    return "unknowable" if shallow else "unknown"        # guarded
if git(root, "merge-base", "--is-ancestor", sha, "HEAD").returncode == 0:
    return "ok"
return "unreachable"                                     # NOT guarded
```

Both answers can be NO for a reason about the checkout rather than about the
commit, and a shallow clone produces each. The object-absent one was covered;
the history-is-cut one was not, and that is the one CI produced.

### Why the objects were present at all

A fresh `--depth 1` clone would have failed `cat-file` and been reported as
unverifiable — the guard would have worked. What makes this lane different is
measured from the run's own log (`run-matrix` 35964714561, job 107520646574):

* the job is `runs-on: [self-hosted, …]`, so `/home/runner/_work/nano-ros/
  nano-ros` is a **persistent workspace**. Its previous HEAD was
  `gh-readonly-queue/main/pr-1246-…`, i.e. a much earlier run's checkout.
* `actions/checkout` runs `git clean -ffdx`, which removes FILES and never
  objects, then `git fetch --no-tags --prune --depth=1 origin +<sha>:refs/
  remotes/origin/main` (1.7 s).

So every earlier run's tip stays in the object store while the history between
them does not. `cat-file` succeeds on exactly the commits a recent phase doc
cites — which is why it was 3 of 3 and not 12 of 12 — the absent-object guard
is bypassed, and `merge-base --is-ancestor` answers a confident false. The
workspace's own memory is what turned a skip into a wrong verdict.

Reproduced synthetically: fetch `--depth 1` at the old tip, fetch `--depth 1`
again at the current one, run the gate. Pre-fix it reports `FAILED: 1 of 1 …
not an ancestor of HEAD` about a commit that is on main; post-fix it reports
`PARTIAL -- … NOT VERIFIED`.

### What landed

* `scripts/lib/git_history.py` + `scripts/lib/git-history.sh` — ONE spelling of
  the rule: truncation can only manufacture a FALSE negative, so a positive
  counts from any clone and a negative from a truncated one is UNKNOWN.
* `check-roadmap-commit-refs` asks it, and has three outcomes. NOT VERIFIED is
  rc 78, which its recipe records in the `nros_check_skip` ledger, so the
  lane's closing line names it instead of "Fast checks passed!" standing for
  it.
* The sweep, because the rule already had two spellings and the second one
  drifted: `check-doc-commit-citations` (its skip was correct but silent —
  issue 0650's shape), `check-submodule-drift` (a shallow submodule clone was
  reported as DIVERGED, telling the reader to rebase work that needs none),
  `submodule-pins-check` (the absent-object arm was guarded, the cut-history
  one reported a fast-forward as a REWIND — a third mutation in its selftest
  now covers it), `docker/can-demo/run.sh`, and
  `check-play-launch-parser-ref`, which had the rule RIGHT and is where it was
  lifted from.
* `check-ancestry-truncation` — the gate the paragraph above asked for. A
  tracked file that invokes `merge-base --is-ancestor` must reach the helper
  or be exempt with a reason. Verified against the pre-fix file: it flags it.

### Why not `fetch-depth: 0`

Priced rather than preferred. The superproject is 12,413 commits / 998 MiB
packed; deepening this replica over local `file://` transport took **49 s**
against the **1.7 s** the run's own `--depth=1` fetch took, and the network
figure is worse. `check fast` runs in EVERY lane, so the cost lands on every
lane rather than on the one that wanted the answer.

A targeted fetch of just the cited shas does not work at all: proving `X is an
ancestor of HEAD` needs the PATH from HEAD back to X, which for a citation a
few weeks old is thousands of commits. There is no cheap version of option 1 —
only the full one.

And a truncated checkout is a legitimate state that a gate on the fast line
will keep meeting. Saying so is the fix that generalises; deepening one lane
is the fix that waits for the next lane.
