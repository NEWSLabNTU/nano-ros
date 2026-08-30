---
id: 922
title: "Lane skippability was keyed on artifact-root sharing rather than on the
  ATTRIBUTION ROUTE, so the 0828 fix over-built 86 rows per lane"
status: resolved
type: bug
area: testing, tooling
related: [0828, 0517, 0482, phase-340, phase-383]
---

## Problem

Issue 0828 was an UNDER-build: `--coords-from` omitted rows whose coordinate was
out of lane, including rows the RUN cannot skip, so `lane=tier2` built a lane
whose own tests then failed on staleness the lane never promised.

The fix keyed skippability on **artifact-root sharing** — a row sharing its root
with a sibling is unattributable by path, so it fails closed and runs at every
lane. That is the right shape and the wrong predicate. Skippability is a
property of **how the run attributes an artifact back to a row**, and root
sharing only answers that for one of the two tables and only half-answers it
there:

* **`[[workspace_fixture]]` rows are attributed by `id`**
  (`attribute_workspace_id`), never by path. Their roots are shared constantly
  and BY DESIGN — 66 of 110 rows sit on 12 shared roots, because a workspace
  builds all its entries into one tree — and none of that reaches attribution.
  The root rule called 84 of them unskippable where the run skips 46.
* **A tie at the SAME coordinate is not ambiguous.** `attribute_path` resolves
  it, because the coordinate is the only thing the caller asked for and every
  tied row gives the same answer. Two `zenoh` rows of one leaf are such a tie;
  the root rule counted them as unattributable anyway — 20 more rows.

Measured on a 1-wise (tier-2-shaped) cover of 15 coordinates:

| variant | fixture | workspace | total |
| --- | --- | --- | --- |
| the 0828 bug (omit fail-closed rows) | 58 | 12 | 70 |
| 0828 fixed via root sharing | 113 | 78 | 191 |
| route-aware + coordinate-aware | 93 | 12 | 105 |

86 rows per lane, built and never run. 0828 was under-building; a blanket root
rule turns it into over-building on the other table, and a middle tier exists to
be affordable.

## Why it was not caught

`build_and_run_select_the_same_{fixture,workspace}_rows` exist precisely to
assert that the build set and the run set are the same rows. They passed,
because `run_side` modelled the run as **pure coordinate membership** — which
matched the build only while the build ALSO ignored fail-closed rows, i.e. only
while 0828 was unfixed. The test was build-vs-a-model-of-the-run, and the model
encoded the bug. Fixing 0828 made it go red against a build that had just become
correct (139 built vs 92 modelled).

A first attempt shipped skippability as a `lane_skippable` COLUMN on the
`coords` record so both sides read one computation. That made the test
TAUTOLOGICAL: mutating `row_is_lane_skippable` moved both sides together and the
negative control passed. A cross-check whose halves consume one computation
cannot catch a bug in that computation — the same lesson as
`check-rmw-api-parity` vs `check-rmw-abi-shape`, which are two green tools kept
deliberately independent.

## Fix

* `row_is_lane_skippable(entry, all_entries, kind)` — workspace rows are
  attributed by a unique `id`, so they are always skippable; the root rule
  applies only to `[[fixture]]` rows.
* `_shared_artifact_roots` counts a root as ambiguous only when the rows sharing
  it carry DIFFERING coordinates, mirroring `attribute_path_in`.
* `run_side` derives skippability from the PRODUCTION attribution functions
  (`attribute_path_in`, `attribute_workspace_id`) rather than from a column the
  build also reads, keeping the two halves independent.

## Verification

Both negative controls now fail in the correct direction, and each is caught by
the table it belongs to:

* omit fail-closed rows (the 0828 bug) → `..._fixture_rows` FAILS
* judge workspace rows by root sharing → `..._workspace_rows` FAILS

`cargo nextest run -p nros-tests --test lane_run_narrowing` → 7/7 pass restored.

## Residue

`row_artifact_root`'s docstring still says an empty root makes `fixtures::lane`
"fail closed on an empty root (never skips)". Issue 0713 moved west leaves onto
`require_west_leaf_in_lane`, so they ARE coordinate-narrowed and skippable; the
docstring predates it. Corrected in the same change.

Neither rule models `attribute_path`'s LONGEST-match tie-break, where a deeper
root clears an ambiguity among shorter ones. No row exercises that today (the
two sides agree exactly), so it is stated rather than implemented — a third
derivation with no live case is how a predicate drifts.
