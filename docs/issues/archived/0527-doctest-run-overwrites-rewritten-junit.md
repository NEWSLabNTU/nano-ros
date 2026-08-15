---
id: 527
title: "The doctest run overwrites the rewritten junit.xml, so a failed sweep's real failures cannot be named afterwards"
status: resolved
type: tech-debt
severity: medium
area: testing
related: [issue-0499, phase-214]
resolved_in: phase-340
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

## Fixed 2026-08-15 — snapshot the artifact, and name the failures

Both of the cheap directions above, because they fail differently: one keeps the
XML, the other survives even when the XML is gone.

1. **`_rewrite-skipped-junit` snapshots to `target/nextest/default/junit-real.xml`.**
   `junit.xml` is written by EVERY `cargo nextest` invocation — not only the
   doctest phase this issue named, but every suite a human re-runs while
   triaging. `junit-real.xml` is written by the rewrite step alone.

2. **`_name-real-failures` prints the ids** (`scripts/test/name-real-failures.py`),
   and all three `test*` recipe tails now call it where they previously printed
   only a count. It reads the snapshot by preference and falls back to the live
   file. It re-derives "real" from the `[SKIPPED]` marker rather than trusting
   the rewrite to have run, matching `_count-real-failures`'s own
   defence-in-depth — the two must not disagree about what real means.

### The author walked into this issue while it was open

Filed in the morning; that afternoon a tier-2 sweep reported 171 failures, I ran
individual suites to triage, and then read `junit.xml` and found
`tests=1 failures=1` — my own solo runs had overwritten it. The count I
eventually trusted (1 real failure, 170 skips) came from re-running the whole
sweep with a manual `cp` of the XML. That is the cost this issue describes,
paid by the person who wrote it down.

### Verified

Synthetic junit carrying one real failure and one `[SKIPPED]`:

| step | result |
| --- | --- |
| namer before rewrite | names the real one only |
| rewrite | 1 `[SKIPPED]` → `<skipped>`, snapshot written |
| `_count-real-failures` | 1 |
| namer after rewrite | same one name — count and names agree |
| **overwrite `junit.xml` with a clean run** | live file says 0; **snapshot still names it** |
