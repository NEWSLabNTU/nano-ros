---
id: 1016
title: "`lane=tier2` does not build the zephyr rust/c west leaves, so their cells
  report as failures that are really skips"
status: open
type: bug
area: testing, zephyr, ci
severity: medium
related: [issue-0968, issue-0828, issue-0445]
found: 2026-09-03
---

## Measured

After a clean `just build-test-fixtures lane=tier2` (module verdict
`== zephyr == OK`), running the nine `example_e2e` zephyr XRCE cells:

```
Summary [295.800s] 9 tests run: 0 passed, 9 failed, 45 skipped
```

Six of those nine had not run at all. They panicked in `resolve_example`
(`zephyr.rs:143`) with:

```
[SKIPPED] zephyr/c/listener xrce image not prebuilt or stale …
  BuildFailed("Zephyr fixture is STALE — a source is newer than the built binary:
    binary: …/build-c-listener-xrce/zephyr/zephyr.exe
    newer:  …/examples/zephyr/c/listener
    probe:  examined 0 input(s); …
    NOT RUN: 3th consecutive stale verdict for this fixture")
```

Bare `cargo nextest` counts a `nros_tests::skip!` panic as a FAILURE (CLAUDE.md
says so), so the summary line for "six were never built" is character-for-
character the same as the summary line for "six ran and failed".

`just build zephyr` (the west-leaf build) does cover them; after it, the same
nine ran with **zero** skips.

## Why it matters more than an inconvenient skip

This is the tier-2 half of issue 0828's class. That one was the build omitting
rows the run would NOT skip, so a stale fixture passed the freshness gate. This
is the run reaching for rows the tier-2 build never made, and the resulting
message being indistinguishable from a result.

It cost a full wrong reading in issue 0968: the six were reported as failures,
reasoned about as failures, and only the `zephyr.rs:143` frame in the panic text
distinguished them. A reader who trusts the `9 failed` count — which is what a
CI summary shows — gets six phantom results.

Note also `probe: examined 0 input(s)` with the leaf DIRECTORY as the "newer"
input. Whether that probe is right is a separate question (issue 0445's family),
but a probe that examines nothing and still returns STALE is worth a look while
here.

## Two candidate fixes, and they are not equivalent

1. **Make the lane build what the lane runs** — the phase-340 W3 property, that
   build-set and run-set are one predicate on one coordinate file. If the west
   leaves cannot be attributed to a tier-2 coordinate they should be in the run
   set at every lane (the `row_artifact_root()` fail-closed rule), which is what
   already happens for other unattributable rows.
2. **Make an unbuilt fixture distinguishable from a failed one** at the summary
   level, not only in the panic text.

Both are worth doing; (2) is what stops the next wrong reading even when (1)
regresses.

## Acceptance

* [ ] `lane=tier2` either builds the zephyr rust/c west leaves or the run does
      not attempt them.
* [ ] A run whose cells were never built cannot report a count identical to one
      whose cells ran and failed.
