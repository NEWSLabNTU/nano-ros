---
id: 1314
title: "`lane_build_covers_run::a_native_build_satisfies_the_tier1_run` failed when
  run directly — a defect, or an unmet precondition of how it was invoked?"
status: open
type: bug
area: testing, ci
severity: low
related: [issue-0828]
---

## What was seen

On 2026-09-11, in the worktree for branch `test/rv-virt-threadx-c-workspace`
(PR #936), `packages/testing/nros-tests/tests/lane_build_covers_run.rs` ran
10/11. `fn a_native_build_satisfies_the_tier1_run` (line ~330) failed. The
agent's diagnosis:
- `CiLane::run_scope` hard-codes `Tier1` to COORDINATE scoping;
- the lane script refuses to run when `NROS_TEST_COORDS` is unset;
- so a test asserting "a native build satisfies the tier-1 run" cannot pass in
  an environment that does not set it.

Neither reads `matrix::CELLS`, so the PR's cell additions did not cause it.

## The contradiction to resolve first

In two OTHER worktrees the same day, `just test-lane-contracts` passed 17/17
(branches `fix/1284-backing-meets-default` and `fix/1285-followup-rtos-
substring`). So either:
- this test is not in that recipe's set, in which case it is in no lane, which
  is itself the issue-1226 shape; or
- it is, and passes when invoked through the recipe, in which case the direct
  run lacked a precondition the recipe supplies, and the only defect is that it
  fails with a confusing message instead of a `skip!` naming what is missing.

Establish which before changing any code.

## Fix, by outcome

- **In no lane:** put it in one, or delete it. `check-default-gates-run-somewhere`
  should have caught that; if it did not, its scope is narrower than its rule.
- **Precondition:** make the test state its precondition with
  `nros_tests::skip!` or a hard failure that names `NROS_TEST_COORDS`, per
  "tests must fail on unmet preconditions".
- **Real contradiction:** CLAUDE.md says tier 1 narrows its run by NAME
  (`NROS_TEST_SCOPE`) and tiers 2 and nightly by COORDINATE. If `run_scope` now
  says `Tier1` is coordinate-scoped, then either the doc or the code is stale.
  Fix the stale one, and cite issue 0828.

## Acceptance

Record the outcome in this issue, with the test green or explicitly skipped
under every supported invocation (`just test-lane-contracts`, bare `cargo test
--test lane_build_covers_run`, nextest).
