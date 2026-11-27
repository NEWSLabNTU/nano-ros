---
id: 3006
title: "A committed count every PR edits merges clean and wrong: two PRs bumping one crate's unsafe census row leave one increment uncounted, and the queue ejects the second"
status: resolved
resolved: 2026-11-27
resolved_in: "the change that archives this issue"
type: bug
area: [ci, gates]
severity: medium
related: [issue-0883, issue-0884, issue-1236, phase-472, phase-591]
found: 2026-11-27
---

**Home: [phase-591](../../roadmap/phase-591-ci-feedback-latency.md)**, whose
subject is how long a red takes to reach its author: a queue ejection for a
defect neither PR has costs a whole batch.

## What happens

`.config/unsafe-census-baseline.txt` is generated
(`scripts/check-unsafe-census.py --write-baseline`) and committed. A PR that
adds one `unsafe {}` block to `nros-node` rewrites the row's `block` column,
for example from 30 to 31.

Two PRs that each add one block to the same crate each make that SAME edit
(30 to 31). A three-way merge treats identical edits on both sides as clean,
so the second PR rebases clean server-side and the result reads 31. The tree
now holds 32 blocks, so `check unsafe-census` fails in the merge group, and
the second PR is ejected for a defect neither PR has.

Observed: #4884 on 2026-11-27. #4846 and #4955 are both about to repeat it on
`nros-node`.

It is 0883/0884's shape (a committed generated file every PR touches
serialises the queue), with a worse outcome. 0883's file CONFLICTED, which
is loud. This one merges, and the number it produces is wrong.

## The second motivating case: a baseline resolved mid-rebase

Rebasing #4846 (3 commits, +6 `nros-node` blocks) stopped at a conflict in
the baseline. The commit editing it (#4846's `wait_for_service` fix, an `nros-cpp` shrink)
comes before the commit that adds the blocks (its action-client fix). At that stop
the tree measures the old count, so a row resolved from that tree, or by
picking either side of the conflict, records a number that is wrong once
the rebase finishes. It is either 6 short, or it carries a shrink against
code the stop has not applied.

The rule today: **resolve a baseline conflict by regenerating it after the
rebase completes**, at the tip, never mid-rebase. With fragments there is
nothing to resolve, because no commit edits a shared row.

## How often

Commits on `main` touching each committed `.config/*baseline*` file,
2026-11-13 to 2026-11-27: unsafe-census 81, prose-issue-ref 20,
gate-registry 14, every other file 4 or fewer. The census is the hot one.
The other two share its shape and become hot as their rules gain sites.

## Options studied

1. **Per-crate baseline files.** No fix: the collision is two PRs in the SAME
   crate, which would still edit one file identically.
2. **Derive the count at merge_group time** (no committed number, compare
   against the merge base). This loses the ratchet's direction. A shrink must
   be RECORDED in the change that makes it (phase-472 W9), so the slack cannot
   regrow unobserved, and a base-relative count forgets a shrink one merge
   later. It also makes every PR's own CI answer a different question than
   the queue's.
3. **A ratchet over the base plus a sum of increments, carried as fragments**
   (recommended). A PR that moves a count writes
   `.config/unsafe-census.d/<slug>.toml` instead of editing the baseline:

   ```toml
   [nros-node]
   kind = "block"
   from = 30      # the baseline row when the fragment was written
   delta = 1      # this PR's own change, never its measured total
   reason = "the ISR wake path's port signal (phase-579 W3a)"
   ```

   `check-unsafe-census.py --write-fragment <slug>` writes it, with `delta` =
   measured − (baseline + Σ the fragments already on the branch). A count is
   not enough: a PR branched after another PR's fragment landed measures that
   increment too, and a `to = 32` would count it twice.

   The gate allows `baseline + Σ delta`, per crate per kind, and still
   refuses a count above that and a fall not recorded (a negative fragment).
   Fragments have unique names, so server-side rebases never conflict and
   never coincide. Two `+1`s are two files, and their sum is 2. A fold
   (`check-unsafe-census.py --fold`) writes the sums into the baseline and
   deletes the fragments in one commit. It is part of the release step, so the
   baseline is exact at every tag.
   - **A stale fragment is refused.** A fragment's `from` must equal the
     baseline row it applies to. After a fold, a fragment written against the
     pre-fold row (a PR branched before the fold, landing after) names a
     `from` the baseline no longer holds, so the gate refuses it with
     "regenerate your fragment" rather than counting an increment that may
     already be folded in. Without that, a stale `+1` would stand as slack,
     and a real new `unsafe` could hide inside it.
   This is towncrier's answer to the changelog, which had the same shape.

   - It keeps every property the ratchet has today: measured counts, a
     shrink-only direction, and a new crate's unsafe visible.
   - It adds one: each increment carries its REASON, so the census finally
     records why it grew, which a number cannot (issue 1236 wanted
     direction, not only a total).
   - The selftest gains the concurrency case: two fragments of `+1` on one
     row against a tree with +2 is clean; against +3 it fails; a fragment
     whose `from` the baseline no longer holds fails.

## Headroom: measured, none

A row above its measured count would be slack that absorbs a later increase
unrecorded. On 2026-11-27, `main` at `edb25d59a0` has 211 nonzero rows, with 0
above the measured census and 0 below.

A reading of "nros-node row 942, measured 942" after rebasing #4846 (which
adds 6 blocks) was a rebase stopped part-way. The commit that edits the
baseline (#4846's `wait_for_service` fix) conflicts on it, and the stop comes before the commit
that adds the blocks (its action-client fix). Reproduced in a throwaway worktree.

The gate's rules keep rows exact: a fall in a crate the branch touched fails
(phase-472 W9), and a crate's count moves only when its own sources change.
The fragments design keeps that, and `--fold` refuses unless the census equals
`baseline + fragments` in both directions.

## Fix the class

The fold helper belongs in `scripts/lib/ratchet.py`, beside `judge`, so
prose-issue-ref and gate-registry can take fragments as their own collisions
appear. They are not migrated pre-emptively: neither has measured an
ejection.

## Acceptance

- Two PRs, each adding one `unsafe {}` to one crate with a fragment each,
  land through the queue in either order with no ejection.
- A PR whose fragment under-states its increase fails, as today.
- After `--fold` the baseline equals the measured census, and the fragments
  are gone. The release step runs it.
- A fragment written against a pre-fold baseline row is refused.

Design approved 2026-11-27 (manager): fragments, baseline + sum, a fold in
the release step, and stale-fragment refusal.

## Resolution (2026-11-27)

`scripts/check-unsafe-census.py`, as designed above:

- `--write-fragment <slug> --reason TEXT` writes
  `.config/unsafe-census.d/<slug>.toml`. `delta` = measured − (baseline +
  the fragments already on the branch), over the crates the branch touched.
- The gate judges the census against `baseline + Σ delta`. It refuses a
  fragment that is malformed, has a zero delta, has no reason, or is STALE
  (its `from` is not the baseline's current row).
- `--fold` writes the census into the baseline and deletes the fragments.
  It refuses while the census and `baseline + fragments` disagree, so the
  baseline is exact. `just release-bump` runs it, and its
  `git commit -am` takes the deletions.
- `--write-baseline` refuses while fragments exist, and the failure text
  names `--write-fragment`.

Tests: the self-test grows to 27 cases. Two concurrent `+1` fragments allow
+2 and refuse +3; fold agrees and refuses; a stale fragment is refused; a
zero delta and a missing reason are refused; the writer records the delta,
not the total. Mutation: with the stale check disabled, the stale case
fails.

End to end on the real tree: one `unsafe {}` added to `nros-node` makes the
gate fail (939 -> 940). `--write-fragment` writes `from = 939, delta = 1`, and
the gate passes. With the block removed, `--fold` refuses (940 against 939).

