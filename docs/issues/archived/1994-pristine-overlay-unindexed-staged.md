---
id: 1994
title: "`check::pristine-checkout` overlays staged adds and `git mv` onto the clone's files but not its index, so git-asking gates measure a different tree"
status: resolved
type: bug
area: [ci, gates]
severity: low
found: 2026-10-19
related: [1373, 1465, 1949]
---

## Measured

The issue 1949 branch moved its own issue doc with `git mv` and ran `just ci gate`
before committing. `check::pristine-checkout` (step 4) went red; the same tree
went green once committed, with no other change.

`scripts/ci/pristine-checkout-fast.sh` builds a `--local --shared` clone of HEAD
and overlays the uncommitted edits with `git diff --binary HEAD | git apply`.
Plain `git apply` writes the WORKING TREE only. Reproduced on a scratch clone,
with one `git mv` and one staged new file:

| overlay | renamed-to path in `ls-files` | staged new file in `ls-files` | renamed-from path in `ls-files` |
| --- | --- | --- | --- |
| `git apply` | absent | absent | **still listed** (file gone from disk) |
| `git apply --index` | present | present | absent |

So every gate that asks `git ls-files` / `git grep` (the documented way to ask
what the tree holds) saw a tree that is neither HEAD nor what a commit would make.
The untracked half of the overlay already had the fix — `git add -N` in the
clone, written for exactly this symptom — but a file that is STAGED is not
`--others`, so it took the tracked path, which had none.

## Resolution (2026-10-21)

- The tracked overlay applies with `git apply --index`, so adds, deletes and
  renames reach the clone's index as a commit would record them.
- A normal-path check follows it: the clone's
  `diff --cached --name-status --no-renames HEAD` must equal this tree's
  `diff --name-status --no-renames HEAD`, path for path, or the lane exits 2
  naming the difference. A future overlay change that drops a class of edit
  fails there rather than as an unexplained gate red.

Verified: `just check pristine-checkout` green with a staged `git mv` and two
staged new files uncommitted. Mutation: the same run with `--index` removed
exits 2 at the new check, listing all five paths missing from the clone's index.
