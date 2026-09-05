---
id: 1057
title: "`ci_lane` unit test is RED on main: tier 2 declares `RunScope::LaneCoords`
  and `just ci matrix` no longer exports `NROS_TEST_COORDS`"
status: resolved
type: bug
area: ci, testing
severity: high
found: 2026-09-04
related: [issue-0828, phase-340, phase-413]
---

## Symptom

On `main` at `ab27c08f2`, with no local changes:

    FAIL  nros-tests ci_lane::tests::recipes_run_the_scope_their_lane_declares

    Tier2 declares RunScope::LaneCoords but `just ci matrix` never exports
    NROS_TEST_COORDS — the run would resolve every coordinate while the
    preflight accepts a `lane=tier2` build

Verified as pre-existing rather than assumed: reproduced with an unrelated
working tree stashed, and again with it restored.

## Why it matters more than one red test

`just check fast` does NOT run unit tests, and the `pull_request` required
context is `check-fast` plus three siblings — `test-unit` runs on `merge_group`.
So this is invisible on every PR and fails in the QUEUE, where an ejection is how
anyone hears about it. That is CLAUDE.md's own "PR cheap, batch thorough" split
working exactly as designed, on a red that nothing pre-merge can see.

## What the test is protecting

Tier 2 narrows its RUN by COORDINATE (`NROS_TEST_COORDS` →
`nros_tests::fixtures::lane`), not by name, because it is 1-wise over platform
and every platform is in it. `CiLane::run_scope` declares that, and this test
asserts the RECIPE actually exports what the declaration promises.

If it does not, the failure mode is issue 0828's, from the other direction: the
preflight accepts a `lane=tier2` BUILD while the run resolves every coordinate,
so rows the lane never built are demanded, and the sweep reports stale-fixture
failures that are an artifact of the lane rather than of the tree.

## Where it came from

`just ci matrix` now dispatches on a `depth` argument, delegating to the private
`_matrix-run` / `_matrix-build` recipes in the `ci` module, so the export moved
out of the recipe body the test reads. (Those two are internal and deliberately
not invocable directly — naming them here as commands is what
`check-doc-recipe-refs` rejects, correctly.) Either the export moved into
`_matrix-run` and the test needs to follow the indirection, or it was dropped in
the split. **Not diagnosed further here** — this issue exists so the red has an
owner and a written cause-so-far, not to guess which.

## Acceptance

`cargo nextest run -p nros-tests --lib -E 'test(recipes_run_the_scope_their_lane_declares)'`
passes on `main`, and the assertion still fails if the export is removed from
whichever recipe now carries it.

## Resolution (2026-09-05)

The first of the two possibilities: the export MOVED, and the test needed to
follow the indirection. `_matrix-run` carries `NROS_TEST_COORDS` on both of its
`just` invocations; nothing was dropped in the split.

`recipes_run_the_scope_their_lane_declares` now follows a recipe's
module-qualified private delegates before asserting, rather than hard-coding
tier 2's inner name — so the next tier that grows a depth is covered by
construction. A delegate that does not resolve fails the test (issue 0196's
rule) instead of silently shortening the body it reads.

Mutation-checked: removing every export from `_matrix-run` fails; renaming
`_matrix-run` without updating the dispatcher fails.

Noted while here, NOT fixed: the assertion is "the body mentions the variable",
so removing one of the two exports in `_matrix-run` still passes. That is the
assertion's existing granularity rather than something the fix narrowed, and
tightening it to "every `just` invocation in the body carries it" is a separate
change with its own blast radius across four tiers.
