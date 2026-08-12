---
id: 527
title: "The doctest run overwrites the rewritten junit.xml, so a failed sweep's real failures cannot be named afterwards"
status: open
type: tech-debt
severity: medium
area: testing
related: [issue-0499, phase-214]
---

## Symptom

`just test-all` (and therefore `ci-matrix`) reports a trustworthy COUNT of real
failures and then destroys the evidence for WHICH they were:

```
rewrite-skipped-junit: rewrote 115 [SKIPPED] failure(s) to <skipped> in target/nextest/default/junit.xml
Real failures: 19 / 19 total failures
```

Read `target/nextest/default/junit.xml` afterwards and it contains **zero**
`<failure>` elements: the doctest phase that runs after the nextest phase writes
the same path, and doctests pass, so the file ends up describing a clean run.

The console log is not a substitute. It interleaves `TRY 1/2/3` retries with
terminal verdicts and carries no skip/fail classification — that classification
is exactly what the rewrite computed and then lost. Sorting the log's terminal
verdicts by duration gets close (a lane skip fails in ~0.03 s, a real failure
takes seconds) but over-counts: on the 2026-08-12 sweep the heuristic said 31
where the rewrite had authoritatively said 19.

## Why it matters

This is the artifact you go to when a sweep fails. It has obstructed diagnosis
three times in one session — each time the answer to "which 19?" had to be
approximated from durations, or recovered by re-running suites individually.

A sweep that can tell you *how many* real failures you have but not *which* is
close to useless for triage, and it fails in the direction that looks fine: the
count is right, the file is present and well-formed, and nothing warns that it
now describes a different run.

## Cause

`justfile` `test-all` runs, in order:

1. `cargo nextest run … --profile default` → writes
   `target/nextest/default/junit.xml`
2. `just _rewrite-skipped-junit` → rewrites `[SKIPPED]` `<failure>` entries to
   `<skipped>` in that file, and prints the real-failure count
3. the doctest phase → runs under the same nextest profile and **rewrites the
   same path**

Steps 2 and 3 disagree about who owns the file, and 3 runs last.

## Directions

* Snapshot the rewritten file before the doctests — e.g.
  `target/nextest/default/junit-nextest.xml` — and point triage at that.
* Or give the doctest phase its own profile / output path so the two never share
  one file.
* Or have `_count-real-failures` also EMIT the failing test ids (it already
  computes them to count them), so the names survive in the console log
  regardless of what happens to the XML.

The third is the cheapest and helps even when the XML survives, because the
console log is what a CI reader actually has.

## Not fixed here

Filed while chasing tier-2 blockers; the fix is not urgent for correctness of
the gate (the count is right) but is the difference between a triagable sweep
and a re-run.
