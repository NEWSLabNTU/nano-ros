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

## Fixed 2026-09-04 — the test was reading a dispatcher

The recipe was never wrong. `_matrix-run` exports both
`NROS_FIXTURE_LANE=tier2` and `NROS_TEST_COORDS="$coords"`, from the same
`nros_lane_coords_file tier2` the build and the staleness gate use — the one
computation reaching all three that issue 0368 F8 set up.

What broke is the ASSERTION. It extracts the named recipe's body and stops at the
next column-0 line; phase-413 turned the tiers into dispatchers, so `matrix` is
now a `case` on `depth` that delegates, and the exports moved one recipe over.
The test was measuring a `case` statement and correctly reporting that it
narrowed nothing.

`recipes_run_the_scope_their_lane_declares` now follows a dispatcher into the
private recipes it delegates to, and checks the union. Two helpers carry it:

* `recipe_body` — the extraction, lifted out of the test body so it can be
  tested on its own;
* `delegated_recipes` — `_`-prefixed names only. A tier delegating to another
  PUBLIC tier is the ladder, checked elsewhere; a private delegate is an
  implementation split of the same recipe and inherits its obligations.

One level deep, deliberately: a dispatcher that delegates to a dispatcher is a
shape nothing here has and one this assertion should not quietly accept.

**Mutation-checked.** Removing `NROS_TEST_COORDS` from `_matrix-run` makes the
test fail again, so the assertion still bites through the indirection rather than
having been satisfied by reading more text.

**And the blind spot is now covered.** `a_dispatching_recipe_is_read_through_to_
its_private_delegate` exercises the extraction on a SYNTHETIC justfile — a test
whose only evidence is the tree it runs in goes green the moment that tree
changes shape again, which is exactly how this went red and stayed red.

## What made it expensive, and it is not the bug

The red was invisible where anyone would look. `just check fast` runs no unit
tests, so neither `check-fast` nor any other required `pull_request` context
could see it; `test-unit` runs on `merge_group`, so it failed in the queue, where
an ejection is how you hear about it. That split is deliberate and documented
(CLAUDE.md's "PR cheap, batch thorough"), and the cost it accepts is exactly this
shape: a red only the queue can see is a red nobody is looking at.

Found by running `nros-tests --lib` while verifying unrelated work, not by the
lane that owns it.
