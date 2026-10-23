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
