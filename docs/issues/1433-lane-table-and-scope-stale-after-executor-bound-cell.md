---
id: 1433
title: "`test-unit` is red on `main` — a new runtime cell landed without its
  lane-table row or a `lane_scope` classification, so two tests fail for it"
status: open
type: bug
area: [testing, ci]
severity: medium
found: 2026-09-21
related: [issue-1226, issue-0630, issue-0357, issue-1314]
---

## What fails

`cargo test -p nros-tests --lib` on `origin/main` (measured 2026-09-21, at
`56db6302c`): **215 passed, 2 failed.**

```
---- ci_lane::tests::documented_lane_table_is_live stdout ----
assertion `left == right` failed: Tier1: the ci_lane module table says 19 cells;
recomputed 20. Update the table (and the justfile comments that quote it).
  left: 20
 right: 19

---- lane_scope::tests::every_cell_iterating_test_is_classified stdout ----
these tests iterate platform-varying cells and are in neither
`lane_scope::CONSUMERS` nor `EXEMPT`: ["native_example_executor_bound_node_e2e.rs"]
```

Both name the same newcomer, and both are correct.

## Cause

`9069d5164` — *"test(#1384): the custom-platform demo gets a runtime cell, and
stops reporting success when it failed"* — added a cell to
`packages/testing/nros-tests/src/matrix.rs` and a new cell-iterating test,
`tests/native_example_executor_bound_node_e2e.rs`. Its six touched files are
`examples/fixtures.toml`, `examples/native/c/custom-platform/src/main.c`,
`checker.rs`, `matrix.rs`, `output.rs` and the new test.

It touched neither of the two registries a new cell has to join:

* `packages/testing/nros-tests/src/ci_lane.rs` — the documented tier-1 cell
  count, still `19`, now recomputing `20`.
* `packages/testing/nros-tests/src/lane_scope.rs` — `CONSUMERS` / `EXEMPT`. The
  test's own failure text states the fix and why it matters: a cell-iterating
  test cannot be narrowed by a name filter (issue 0357), so under
  `NROS_TEST_SCOPE=native` it reaches every platform's cells and a missing
  non-host fixture PANICS rather than skips (issue 0584/0630).

`git merge-base --is-ancestor 9069d5164 origin/main` — yes, so this is on main,
not in anyone's branch.

## Why it was not caught

`test-unit` runs on `merge_group`, not on `pull_request` (CLAUDE.md's "PR cheap,
batch thorough"), so a green `CI` on the PR never asked. The lane that does ask
is `just ci gate`, which CLAUDE.md tells every contributor to run before every
push — so the cost is paid by whoever runs it next, on somebody else's defect.
That is the issue-1226 class exactly, one lane over: a gate that WORKS, in a lane
whose verdict arrives after the merge.

Noticed from an unrelated branch (issue 1428) whose diff touches only manifests,
a gate script and a justfile — none of `ci_lane.rs`, `lane_scope.rs`,
`matrix.rs` or `fixtures.toml`.

## Fix

Update the tier-1 count in `ci_lane.rs` (and the justfile comments that quote
it), and either add the `lane_scope::admits(c.platform)` guard to the new test's
cell loop and list it in `CONSUMERS`, or put it in `EXEMPT` with the reason it
needs no narrowing.

The recurrence question is the more interesting one, and is NOT answered here:
`documented_lane_table_is_live` and `every_cell_iterating_test_is_classified`
both exist precisely to catch this, and both did — after the merge. Whether the
cell registries should be reachable from a `pull_request` lane is the same
trade-off phase-395 priced for `test-unit` as a whole.
