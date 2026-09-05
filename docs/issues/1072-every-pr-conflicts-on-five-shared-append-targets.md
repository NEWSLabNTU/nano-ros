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

---

## RE-MEASURED 2026-09-05, later the same day — the picture moved

Twenty of the 51 pull requests merged between the two measurements, so the
numbers above are stale. Re-run against `origin/main` at `29d8dd61d`, with
`git merge-tree --write-tree` — which **is** available: this host has git 2.55,
and the "git 2.34.1 here, it needs 2.38+" note in the Method section was wrong
about the machine it was written on.

**31 open pull requests, 11 CONFLICTING.**

| conflicting path | in how many of the 11 |
| --- | --- |
| `docs/reference/api-parity-ledger/node.json` | 4 |
| `docs/reference/api-parity-ledger/other.json` | 3 |
| `docs/roadmap/phase-424-build-graph-freshness-truth.md` | 3 |
| `docs/reference/api-parity-ledger/graph.json` | 2 |
| **`just/check.just`** | **2** |
| seven other paths (one issue file each, two executor sources, one cmake) | 1 |

As a group, `docs/reference/api-parity-ledger/*.json` is the largest cluster —
4 distinct pull requests (#329, #446, #471, #481). `phase-424` is 3 (#434,
#472, #492). `just/check.just` has dropped from 12 to 2.

### Conflict count is the wrong lead indicator, and the touch count says so

Two more measurements, because "conflicts against main today" only counts the
hazards that have already fired:

**Paths the most open PRs TOUCH** — the pool every future conflict comes from:

| path | open PRs touching it |
| --- | --- |
| **`just/check.just`** | **15 of 31** |
| `packages/core/nros-node/src/executor/spin.rs` | 11 |
| `packages/api/nros-cpp/src/lib.rs` | 8 |
| `packages/api/nros-cpp/include/nros/node.hpp` | 6 |
| `docs/reference/api-parity-ledger/init.json` | 6 |

`just/check.just` is touched by nearly half of all open pull requests, 36 %
more than the next path. Its conflict count is low today because most of those
15 have not yet had `main` move under their particular anchor — not because
the hazard went away.

**Merge-queue simulation** — for each of the 20 PRs that are mergeable now,
land it on `main` and then merge each of the other 19 (`git commit-tree` on the
merge-tree result, then a second merge-tree). Of 190 pairs, **6 collide**, on
`phase-412`/`boot_report.rs`/`spin.rs`/`task.rs` — none on `just/check.just`.
Two independent appends to that file usually *do* merge; the question is what
happens when they do not.

### §1 was wrong: the registry is not innocent

The issue says above that the `fast-serial:` registry "merges **cleanly**" and
that only the recipe bodies conflict. Both live `just/check.just` conflicts
were examined; they are one of each, and the registry one is real:

* **#472** conflicts **in the registry**, at one hunk of three lines. It adds
  `codegen-stamp-inputs` and `codegen-tool-reconfigure`; `main` had already
  landed `codegen-tool-reconfigure`. Both insert at the same base index (42),
  git has no unchanged line between them to anchor on, and it conflicts.
* **#471** conflicts **in a recipe body** — but not by appending at a shared
  anchor. Both sides *rewrote the same block* of `check-c` (one adding a
  serialization-format probe, the other retiring the deprecation probes). That
  is a genuine semantic disagreement about what the gate checks, and no
  positional rule fixes it.

The "four lines apart produced no conflict" observation was right and does not
generalise. A sorted list is conflict-free only when the two new names are far
enough apart, and **gate names are not independent**: related work produces
related names, so sorting converts name correlation directly into position
correlation. Measured across the 8 open PRs that add registry entries, the
insertion indices are 15, 15, 15, 42, 42, 84, 139, 151, 208, 209, 209, 210,
210 — clustered, not spread.

### Why "sort the recipe bodies" (proposal 1 above) was rejected

1. **It fixes neither measured conflict.** #472 is the registry. #471 is two
   rewrites of one block, which relocating does not help.
2. **It makes the two conflict classes correlated instead of independent.**
   Sorting bodies by name puts `codegen-stamp-inputs`'s body immediately
   beside `codegen-tool-reconfigure`'s *by construction* — the same
   name-adjacency that already collides in the registry, now also colliding in
   the bodies.
3. **The cure costs more than the disease.** Reordering ~4,000 lines conflicts
   with all 15 in-flight pull requests at once, converting a latent hazard into
   15 certain conflicts.
4. It scatters gates that share a comment block and a subject.

## FIXED (the `just/check.just` half)

The lesson from 0883/0884 is that the only fix which reaches the merge queue
removes the **shared authored line** — a `.gitattributes` driver does not run
on GitHub's server-side rebase. So the registry was not reformatted; it was
deleted.

**A recipe in `just/check.just` is a fast-lane gate unless it is listed in
`build-serial:`, named in `.config/gate-lane-exempt.txt` with a reason, or
declares parameters** (a gate runs as `just check <name>`, with nothing after
the name, so a recipe requiring an argument can never be one).

* The 218-name `fast-serial:` dependency list is **gone**. Adding a gate is
  writing its recipe: one insertion, in the author's own region of the file,
  colliding with nobody. The 13 registry additions across the currently open
  PRs become 0.
* `fast-serial:` keeps its name and its meaning — the same set, one gate at a
  time, fail-fast — via `run-gates-parallel.sh --serial`, so both spellings
  answer from one derived list rather than two copies.
* `build-serial:` stays an authored dependency list, deliberately: ~20 names
  that change rarely (none of the 13 additions went to it), read directly by
  `check-gate-visibility`, and a gate belongs there only when it *cannot* run
  without something built — worth stating explicitly.
* `.config/gate-lane-exempt.txt` holds the 33 recipes that are not fast gates,
  each with a required reason. It is a ratchet in spirit, like
  `.config/ungated-gates.txt`: it should shrink.

### This also closes issue 1071's other half

PR #431 lost four gates by deleting their registry lines while the recipes
stayed, and `check-gate-lists` — which verified the list's SHAPE — stayed
green. There is no fast-lane registry line to delete now. A fast gate can only
leave the lane by having its RECIPE deleted, which is visible in review.

The polarity flip also inverts the failure mode, in the direction this repo
keeps asking for. Forgetting the registry line meant the gate NEVER RAN —
silent, issue 0196's class. Forgetting to exempt a new recipe means it runs on
the fast lane: loud, and on the author's own pull request.

**The name-set ratchet (PR #493) still works**, and reads one function instead
of a regex: `check_gate_lists.gate_names()` returns the sorted fast + build
set, raising rather than returning a short list if the classification is
broken. `.config/gate-registry-baseline.txt` compares against that. The
baseline's contents do not change — the derived set is byte-identical to what
the old registry declared (218 fast + 21 build = 239).

`check-gate-lists` stays meaningful, and checks something stronger than before:
that the classification is **total and unambiguous** — every name in the build
registry or the exemption ledger resolves to a real recipe, nothing is claimed
twice, a parameterized recipe is never listed as a gate, and every exemption
says why. The old shape checks on `build-serial:` (one per line, sorted,
sentinel last, no duplicates) are unchanged.

### Caught in passing: 12 recipes in no lane and with no caller

Deriving the classification required enumerating every recipe, which turned up
recipes that are in neither registry and that nothing invokes:
`archive-lang-items`, `book-identifiers`, `dist-runtime-deps`,
`executor-stack-floor`, `nextest-test-filters`, `rmw-feature-matrix`,
`sched-matrix`, `stack-floor`, `submodule-drift`, `support-status`,
`workspace-rmw-agreement`, `zenoh-archive`. They are recorded in the exemption
ledger marked `no caller` so they stop being invisible; deciding their fate is
issue 1071's business, not this one's.

It also turned up **5 recipes the old parse never saw at all** — `stack`,
`stack-all`, `stack-c`, `stack-elf`, `tier-priority-plan-image`, all of which
take parameters. Any audit of `just/check.just` written against
`^[a-z][a-z0-9-]*:` has been missing them.

## STILL OPEN, and out of scope for this change

* **`docs/reference/api-parity-ledger/*.json` (4 PRs) is now the largest
  cluster.** The recommendation stands and is unchanged by the re-measurement:
  a merge driver cannot reach the queue, so the untracking-shaped answer is the
  one available — one file per key, or generate the ledgers and gitignore them
  the way `docs/issues/open.md` was. They are written by `scripts/api-parity.py`
  with `sort_keys=True`, so on-disk order carries no information and two
  branches adding disjoint keys collide with certainty.
* **`docs/roadmap/phase-424-*.md` (3 PRs)** — the Status paragraph, as
  described in §4 above.
* **`check-gate-visibility` had a latent vacuity** created by this change and
  closed with it: it read `fast-serial:`'s dependency line, which now returns
  an empty list. It asks `check-gate-lists.py --list fast` instead. Today the
  branch is unreachable (`just check fast` IS run by a gating job), so this is
  defensive — but a gate that silently reports zero is the exact shape that
  file exists to prevent.
