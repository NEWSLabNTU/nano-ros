---
id: 1205
title: "`check-doc-commit-citations` resolves hashes against whatever the local
  clone has FETCHED, so a fetched PR branch makes it demand the removal of
  baseline entries every other clone still needs"
status: open
type: bug
area: [ci, docs]
related: [1178, 0359, phase-419]
---

## What

`scripts/check-doc-commit-citations.py` is a shrinking ratchet: it fails when a
new dangling citation appears **and** when a baselined entry stops offending, so
the debt cannot grow and cannot go stale.

"Stops offending" is decided by `git cat-file --batch-check` against the local
clone. That answers *"is this object in my object store"*, not *"is this commit
in this repository's history"* — and those differ the moment you `git fetch`
anything. A fetched PR branch, another agent's work branch, or a stale
remote-tracking ref all make commits resolvable that a fresh clone cannot see.

The gate already recognises the class in its own docstring and guards only one
direction:

> `git cat-file` can only answer for objects the clone has. A shallow checkout
> would report a live commit as dangling, so the gate SKIPS there rather than
> failing: an answer that depends on clone depth is worse than no answer.

The opposite direction — a clone with **extra** objects — is unguarded, and it
is the dangerous one, because it tells you to DELETE baseline lines. That drives
the ratchet backwards.

## Measured

On a working tree that had run `git fetch origin` (bringing in
`origin/phase-428-w10-qos-ssot`, PR #629's unmerged branch):

```
check-doc-commit-citations: 3 baseline entry/entries no longer needed — the citation resolves now, or the line is gone. Remove them:
  839f69b5c
  d1d88f660
  fe807a999
```

All three resolve locally:

```
839f69b5c  docs(phase-428): file five remainder findings as issues; close out the stale W5 section
d1d88f660  fix(#1055): the box mirror dropped tracked source, so the box could not build nros
fe807a999  feat(phase-428 W6): #[must_use] where a bool reports failure; ...
```

Two of them are commits on PR #629's branch. The baseline says so itself:

> `docs/roadmap/phase-428-api-porting-principle-sweep.md`. ALREADY FIXED on
> PR #629's branch, which replaces all three with descriptions of the commits
> — remove these three lines when that PR lands.

So the gate reports that #629 has landed, because this clone fetched its branch.
It has not landed; it is in the merge queue. Acting on the instruction would
delete baseline entries that every clone without that fetch still needs, and the
next contributor would get a NEW-dangling failure with no baseline to absorb it.

This is why the gate fails locally while CI is green: CI's checkout has neither
the extra refs nor (per the shallow guard) the depth to answer at all.

## Second, smaller finding: one baseline comment is mis-classified

`d1d88f660` sits under the `FOREIGN` heading, described as a "`ros2` distrobox
tree state ... a separate checkout on a separate tree". It resolves to an
in-repo nano-ros commit, `fix(#1055): the box mirror dropped tracked source, so
the box could not build nros`. Either the hash or the classification is wrong.
`FOREIGN` and `DEAD` are treated differently by the file's own rules — only the
second is debt — so a row in the wrong class is a row nobody will ever correctly
retire.

## Fix

Resolve against this repository's own history rather than its object store.
`git merge-base --is-ancestor <hash> <ref>` against the default branch answers
the question the gate means to ask, and a commit reachable only from a fetched
PR branch correctly reads as not-yet-landed. `git rev-list --all` is not a
substitute — it includes fetched remote-tracking refs.

Keep the shallow-clone skip; it guards the other direction and is still needed.

Until then the stale-baseline half is advisory on a developer machine: do not
delete a baseline line because a local run asked you to, without checking
whether the commit is reachable from `origin/main`.

## Second measurement: the gate has no verdict capacity in CI

Prompted by `main` going red on a phase-454 W5 citation (PR #1013) to the
pre-wave tree on its own branch, invalidated by the queue's server-side rebase
— and sitting red for days while every agent read it as environmental. The hash
is deliberately not repeated here: a dangling one written into a live document
is the very thing this gate refuses.

Counted over the live (non-`archived/`) markdown the gate scans: **245 distinct
9-hex citations, 329 occurrences.**

| where the commit is reachable from | count |
| --- | --- |
| `origin/main` | 231 |
| some other ref only (a fetched work branch) | 1 |
| nothing here | 13 |

The 231 cannot decay: the `main-rules` ruleset forbids force-push and deletion
on `refs/heads/main`, so a commit once on `main` stays reachable forever. The 13
are the baseline's dangling entries, and the 1 is `d1d88f660` — this issue's own
false positive, reachable here only because this clone fetched
`origin/work/1055-box-sync-tracked-source`.

So the standing debt is 14 and the inflow has exactly ONE shape: a document
citing a commit on its own unmerged branch. That citation is green on the PR —
the branch is checked out, so it resolves — and dangling the moment the queue
rebases it server-side. The gate can only ever report it after it has landed.

**Switching to default-branch ancestry (the fix above) would catch it on the
PR**, because a citation to the PR's own branch reads as not-on-`main`, and the
gate's existing message ("cite a PR or issue number, or a description") is the
right advice at exactly that moment. The switch is also cheap to price: of the
245 live citations, **exactly 1 changes verdict under it**, and that one is
already baselined. It is a predicate change, not a migration.

**But it buys nothing in CI on its own.** `actions/checkout@v4` is shallow by
default and no gate job sets `fetch-depth: 0` (only `nightly.yml` does), so
`is_shallow()` is TRUE on every `pull_request` and `merge_group` run and the
gate prints `[SKIPPED]` and returns 0. It has never produced a verdict in CI.
That is the real reason a dangling citation can sit on `main`: nothing on the
required lane ever goes red for it, so the only evidence is a developer's local
`just check fast`, where a gate that is red for nobody else reads as
environmental. A gate that always skips is indistinguishable from a gate that
passes — CLAUDE.md's red-lane rule, one level out.

Closing the class therefore needs both halves:

1. resolve against ancestry of the default branch rather than `rev-list --all`
   (this issue's original fix), and
2. give the gate a history to answer from in CI — `gate.yml` already has the
   pattern, in the step that fetches the base ref for
   `check-submodule-pins` precisely because "`actions/checkout` is shallow, so
   `origin/main` is normally absent here" made that gate `exit 0` without
   comparing anything on every PR.

Half 2 without half 1 still misses this defect (on a PR the citation resolves);
half 1 without half 2 never runs. Neither is large.
