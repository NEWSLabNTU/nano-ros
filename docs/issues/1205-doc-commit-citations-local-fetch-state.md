---
id: 1205
title: "`check-doc-commit-citations` resolves hashes against whatever the local
  clone has FETCHED, so a fetched PR branch makes it demand the removal of
  baseline entries every other clone still needs"
status: open
type: bug
area: [ci, docs]
related: [1178, 0359]
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
