---
id: 1433
title: "`test-unit` is red on `main` — a new runtime cell landed without its
  lane-table row or a `lane_scope` classification, so two tests fail for it"
status: resolved
resolved_in: 2026-09-22
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

## Resolution

Fixed by `252718c69` (PR #1148). `cargo test -p nros-tests --lib` on `main`:
**217 passed, 0 failed** — it was 215 passed / 2 failed.

**The new cell was checked before its count was accepted.** Fixing a count to
match a wrongly-declared cell would have been the wrong fix, so the cell was
verified first: its `fixtures.toml` row's coordinate matches the cell exactly,
and `Runtime` is the point of the change that added it (the row had been
BUILD-ONLY, which is what let its publisher answer `NROS_RET_NOT_INIT`
unobserved for four phases). So 19 -> 20 is correct: `ExecutorBoundNode` is a
new value of the **workload** axis and tier 1 covers workload 1-wise, so the
greedy cover had to grow by one. Its coordinate was already held, so the
coordinate count and the cost column did not move.

**EXEMPT, not CONSUMERS — decided from the code.** `is_executor_bound_node_cell`
requires `PlatformId::Linux`, and both the test and its tripwire read
`matrix::CELLS` only through that predicate, so the cell set is the host board by
construction: `admits` would be a literal no-op and no non-host fixture is
reachable, so issue 0630's panic-instead-of-skip cannot arise. That is verbatim
the reason the two sibling native-example tests already carry. A `CONSUMERS`
entry would have claimed behaviour the file does not have — and `silent_consumers`
would then have failed it for never calling `admits`.

**Bonus drift fixed, and it is this gate's own shape.** The module table also
said 36 nightly cells while the gated array asserts 37: a commit on 2026-09-10
fixed the array the test READS and left the prose it claims to mirror, so the
gate sat green over a stale table for 11 days. That makes it narrower than the
rule it enforces (issue 0196's shape) — it asserts the code against a hand-kept
array, not against the table. Corrected, with a note in the module doc so the
next editor moves both.

Sweep: the tier-1 **cell** count is quoted in exactly one place. The `justfile`
and `just/ci.just` quote *coordinate* covers and the *nightly* cell cover —
different denominators, which `ci_lane.rs`'s own doc records as deliberately
ungated prose — and RFC-0061's table is a dated record of measured alternatives.
Neither was touched.

**Why it was not caught before the merge, recorded as a recommendation rather
than acted on:** both tests exist to catch exactly this and both did, *after* the
merge, because `test-unit` runs on `merge_group`, not `pull_request` (issue
1226's class, one lane over). `just ci gate` also passes `--exclude nros-tests`,
so today neither the PR lane nor the local gate asks — which is why this red was
found from an unrelated branch. The suggestion is NOT to move all of `test-unit`
PR-ward (phase-395 priced that at ~3.5 min/push) but to move just the two
registry tests: `documented_lane_table_is_live` and
`every_cell_iterating_test_is_classified` are pure table and source reads,
measured together well under a second, need no fixture, SDK or toolchain, and
would sit beside `check-workspace-all`, which moved PR-ward on exactly this
cost argument.

