---
id: 1072
title: "22 of 51 open pull requests conflicted, none of them on code — five
  shared append targets account for every one"
status: open
type: tech-debt
area: tooling, ci
related: [0883, 0884, 1071]
---

## The measurement

On 2026-09-05, 51 pull requests were open and **22 were `CONFLICTING`**. For
each, the files it touches were intersected with the files `main` moved since
that branch's merge-base. The result is not a spread:

| conflicting path | in how many of the 22 |
| --- | --- |
| `just/check.just` | **12** |
| `docs/issues/README.md` | 5 |
| `docs/issues/0968-tier2-runtime-failures-unreproduced.md` | 4 |
| `docs/issues/1052-esp32-talker-faults-after-network-bringup.md` | 4 |
| `docs/roadmap/phase-424-build-graph-freshness-truth.md` | 4 |
| `docs/reference/api-parity-ledger/*.json` | 3 |

**Not one conflict was a genuine disagreement about code.** Every one was two
authors appending to the same shared file. Nineteen of the 22 were resolved
mechanically — keep both sides, order them by narrative — and the resolution was
the same shape every time.

This is issue 0883's finding, which was thought to be closed. It is not; it
recurred in four more places at once.

## The five, and why each is a different problem

### 1. `just/check.just` — the format fix worked; the recipe bodies never got one

The obvious suspect is the `fast-serial:` registry, and the obvious suspect is
innocent. That list is already hardened: one name per line, sorted, with a
`_gate-list-end` sentinel added specifically so the last entry is not the one
line with different syntax. It merges **cleanly**. Two branches inserting
`preconditions-provisioned` and `prereq-roles` four lines apart produced no
conflict at all.

What conflicts is the **recipe body**. A new gate is ~15 lines of comment plus
three lines of recipe, appended at a spot the author chooses, and there is no
ordering rule for bodies — so authors pick a semantically-related neighbour, and
two people adding related gates pick the *same* neighbour. Both PRs above
appended immediately after the `kconfig-overridden-values` comment block.

The registry got the treatment and the bodies did not, which reads as an
oversight rather than a decision. Sorting bodies the way the registry is sorted
would make two additions land at different offsets by construction.

### 2. `docs/issues/README.md` — frozen, and the in-flight PRs predate the freeze

`main` now carries `## Recently resolved — FROZEN, do not append (issue 0883's
class)`, which says outright that *adding an entry here is the conflict everyone
else then has to resolve*. That is the right fix. But five open branches were
written before it and still add a digest line, so each one now conflicts against
the paragraph explaining why it should not.

Resolution is to drop the row: the digest is a summary of `archived/<id>-*.md`,
which is consistently the larger document (#0870: 41,035 bytes archived vs a
~2,000-byte digest). Nothing is lost. But nothing *tells* the author that —
the freeze is prose, not a gate.

### 3. A live issue file — several sessions investigating one bug

`0968` and `1052` are open issues under active investigation by more than one
session, and each session appends a findings section. Four PRs each on the same
file, all at the same anchor.

Unlike the others this one is **not obviously wrong**. The alternative — one
file per finding — trades a merge conflict for a reader who has to assemble the
narrative from six files. What makes it expensive here is that the sections have
a real ORDER (`gdb names the frame` → `STATIC ANALYSIS` → `RESOLVED, both
halves` → `ROOT CAUSE`), and a positional 3-way merge gets that order wrong
silently: resolving four of #426's six commits by anchor left `STATIC ANALYSIS`
sitting after `CONFIRMED by experiment`, reading as the newest word when it is
the superseded one.

### 4. A phase doc's `**Status**` paragraph and issue table

`phase-424` carries one Status paragraph and an eight-row issue table. Every PR
in the phase rewrites the paragraph and one row. Two sides each rewriting the
same paragraph is not a merge git can do — and taking both is worse than taking
one, because the doc then states two different counts of what is closed. (That
exact defect shipped once already and had to be repaired.)

The table rows are the tractable half: each row belongs to the side that
actually moved that issue, and picking per-row is mechanical once stated. The
Status paragraph has to be rewritten by hand from the merged table, every time.

### 5. `docs/reference/api-parity-ledger/*.json` — a textual merge on a sorted map

These are flat JSON objects written by `scripts/api-parity.py` with
`sort_keys=True, indent=1`. **On-disk order carries no information**, so two
branches adding disjoint keys land at the same byte offset and conflict with
certainty. Union-merging `node.json` gave `116 ours + 120 theirs -> 122 keys, 0
clashes` — a merge git had no way to perform and a five-line script does
exactly.

This is the one with an unambiguous fix: a `.gitattributes` merge driver that
parses, unions, and re-dumps.

**Except it cannot be, and that is the load-bearing part** — issue 0884 already
established that GitHub rebases queue entries **server-side**, where
`.gitattributes` drivers do not run. So a driver fixes local merges and leaves
the merge queue exactly as it is. The fix that reached the queue for `open.md`
was *untracking the file*. Whatever is done here has to work with no custom
driver.

## Why it costs more than the resolutions

The merge queue is serial. A batch ejects when one entry conflicts, and every
entry behind it re-runs. With 22 of 51 conflicting on six files, an ejection is
not bad luck — it is the expected state.

CLAUDE.md already names the class for one file:

> **A generated file that is COMMITTED and touched by every PR serialises the
> merge queue** (issues 0883/0884)

The generalisation the measurement supports is stronger, because four of these
five are not generated: **any file every PR appends to serialises the queue,
generated or authored.** `open.md` could be fixed by generating it and
gitignoring it. `just/check.just` cannot be untracked, `docs/issues/README.md`
is authored prose, a live issue file is the record itself, and a phase doc's
Status is a human sentence.

## What to decide

1. **`just/check.just` recipe bodies** — sort them, the way the registry already
   is, and gate it. Highest count by a factor of two, and the cheapest.
2. **`docs/issues/README.md`** — the freeze is prose; make it a gate, so a PR
   adding a digest row fails at `check` instead of at the queue.
3. **The parity ledgers** — decide the shape given that a merge driver does not
   reach the queue. Splitting one file per key is the untracking-shaped answer.
4. **Phase docs** — the issue TABLE can be per-row mechanical. Whether the
   Status paragraph should exist at all, when the table beneath it already
   carries the state, is the real question.
5. **Live issue files** — probably leave alone. Worth stating the append
   convention (newest section last, and say what it supersedes) so a positional
   merge is less likely to reorder a narrative silently.

## Method

```
# for each open PR: files it touches ∩ files main moved since its merge-base
mb=$(git merge-base origin/main origin/<branch>)
comm -12 <(git diff --name-only $mb..origin/<branch> | sort) \
         <(git diff --name-only $mb..origin/main     | sort)
```

`git merge-tree --write-tree` would answer this directly and is unavailable —
git 2.34.1 here, it needs 2.38+.
