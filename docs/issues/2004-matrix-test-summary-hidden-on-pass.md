---
id: 2004
title: "A matrix test that PASSES over cells it did not run prints which ones, and nextest throws that away"
status: open
type: bug
severity: medium
area: [ci, testing]
related: [0571, 0445, 1758, phase-466]
found: 2026-10-23
---

**Home: [phase-466](../roadmap/phase-466-scheduled-lanes-produce-verdicts.md)** — it is what keeps W5 from being decided.

## Measured

The tier-2 nightly of 2026-10-22 (run 39739466151, job 119692911325) reached
its cells, 3190 tests, and reported:

```
PASS [ 118.874s] (3182/3190) nros-tests::realtime_tiers_e2e realtime_tiers
PASS [  23.194s] (3186/3190) nros-tests::sched_dims_applied_e2e sched_dims_applied
```

Phase-466 W5 asks one thing of that run: did both tests reach a verdict on
every RTOS cell? The log cannot answer it. Each of these tests runs a whole
matrix inside one `#[test]`. A cell outside the run's coordinates is recorded
with `lane_scope::skip_note` and the test still passes when the cells that did
run pass. Issue 0571 made every such test print a summary for exactly that
reason:

```
realtime_tiers: 6 row(s) ran, 0 skipped, 14 out of lane
  - zephyr/rust: out of lane (this run's NROS_TEST_COORDS selects no Zephyr coordinate)
```

nextest discards a passing test's output (`success-output = "never"` is its
default, and `.config/nextest.toml` stores no success output in junit either).
So the summary was printed on every lane and read on none. A green over 20 of
20 rows and a green over 6 of 20 produce the same log line, which is 0571's
defect one layer out.

## Which tests

Derived, not listed: every test file in `nros-tests/tests/` that calls
`skip_note(` for a summary rather than handing it to `lane_skip!`. Today those
are `entry_e2e`, `multihost_e2e`, `realtime_tiers_e2e`,
`roundtrip_xprocess_e2e` and `sched_dims_applied_e2e`.

## Fix

An override in `.config/nextest.toml` sets `success-output = "final"` and
`junit.store-success-output = true` for those five binaries. Gate
`check-matrix-summary-visible` (fast line) derives the subject from the
sources and fails when one is missing from that override. Its self-test
covers the `lane_skip!` exemption and refuses a `test()`-narrowed override as
covering the whole binary. Mutation: dropping `sched_dims_applied_e2e` from the
override fails the gate, naming it.

Verified on a scratch crate with nextest 0.9.137 that the override prints a
passing test's stdout under a profile other than `default` and composes with
an earlier override that sets other fields for the same binary.

## What would close it

The next scheduled tier-2 run that reaches its cells prints both summaries in
its log. That is also the evidence phase-466 W5 is waiting for.

## 2026-10-31 — half landed: the junit carries it, the log still does not

Read off three tier-2 runs (run-matrix 41013529125, nightly 41018247377 and
40833557587). The `junit.store-success-output = true` half works: both
summaries are in the uploaded junit (`post-submit-junit` from `run-matrix`,
`tier2-nightly-junit` from `nightly`), and W5 was read from there. The
`success-output = "final"` half never reaches the log: every lane recipe
that runs these binaries passes `--success-output never` on the command line
(seven sites in `justfile`, `_test-focused` among them, plus the per-platform
recipes under `just/`), and a nextest CLI flag outranks a per-test override. The run-matrix log has the `PASS` line
and no summary, exactly as before. `check-matrix-summary-visible` passes
because it checks the config, which is correct; the flag that defeats it is
in the recipe.

So the close condition above is still unmet. Two ways to meet it, either
enough:

- drop `--success-output never` from those recipes and let the profile
  decide (the override already limits it to the five binaries), or
- have the lane's report step print every `^\S+: \d+ (row|cell)` line it
  finds in the junit's `system-out` into the job summary — the junit is
  already collected by `fixture-tree.sh reports`.

Reading the summaries also showed they miscount (issue 2171, resolved): "22 row(s)
ran" for 4 executed.

## 2026-10-31 — the log half

Took the first of the two ways above. `--success-output never` is gone from
every command line that ran `nros-tests` (32 sites: `justfile`, eight
`just/*.just` recipes, `scripts/feature-lanes.py`,
`scripts/test/check-out-of-lane-deselected.py`); each `--failure-output`
setting is unchanged. With no flag the profile decides, and the profile shows
a passing test's output only for the five binaries the override names, so
every other test is as quiet as before.

`check-matrix-summary-visible` now has a second rule: no tracked
`justfile`, `just/*.just`, `scripts/**` or workflow line may pass
`--success-output never` (or `=never`, or the Python argv pair). Self-test 10
cases; mutation: putting the flag back on `just/xrce.just:91` fails the gate,
naming that line.

What would close it is unchanged: the next scheduled tier-2 run whose LOG
(not only its junit) carries an `executed (` summary line for
`realtime_tiers` and `sched_dims`.
