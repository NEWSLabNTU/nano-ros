---
id: 571
title: "`realtime_tiers` passes tier 1 in 12 s by SKIPPING 15 of its 16 cells —
  build the images and it exceeds nextest's 60 s"
status: open
type: bug
area: testing
related: [issue-0572, issue-0466, issue-0445, rfc-0051, phase-329]
---

## Symptom

Two `just ci` runs, same tree, hours apart:

```
PASS    [  12.310s] nros-tests::realtime_tiers_e2e realtime_tiers
TIMEOUT [  60.013s] nros-tests::realtime_tiers_e2e realtime_tiers
```

Nothing about the test changed between them. What changed is the *tree around
it*: a `lane=tier2` fixture build ran in between, so the embedded QEMU images
exist now. Run directly, with no nextest timeout:

```
test result: FAILED. finished in 66.42s
realtime_tiers: 1 of 16 row(s) FAILED:
  nuttx-arm/rust: high-tier /ctrl counter 0 is not ≥3× the low-tier /telem counter 4
```

## What is actually wrong

`realtime_tiers` is ONE test iterating every `Workload::RealtimeTiers` runtime
cell serially (`tests/realtime_tiers_e2e.rs:461`), booting a QEMU image per
embedded cell. Its wall-clock is therefore a function of **how many fixtures
happen to exist on the machine**, and its verdict is a function of the same:

| images present | cells actually run | time | verdict |
| --- | --- | --- | --- |
| native only | 1 | 12 s | PASS |
| + embedded | 16 | ~70 s | TIMEOUT (and one red cell inside) |

The 12-second PASS is the dangerous half. Tier 1 does not build embedded
fixtures, so on the lane where this test runs it reports green **because the
images are missing** — the absent-fixture case is indistinguishable from the
working case, which is issue 0445's shape at the suite level. A cell can rot
for months and tier 1 will keep saying PASS.

Then, the moment a developer builds any embedded lane locally, tier 1 starts
failing on a 60-second timeout that has nothing to do with their change — which
is how this was found (during phase-351 W3, whose diff is unrelated).

## Why the timeout is the wrong signal

nextest kills at 60 s and prints TIMEOUT. The test swallows per-cell panics
(`catch_unwind` + a silenced hook) and only reports at the END, so a timed-out
run prints **nothing at all** — not the cells that passed, not the one that
failed. The red cell (issue 0572) was invisible until the binary was run
outside nextest.

## Fix — needs a decision, not just a patch

1. **Per-cell time budget + report as you go.** A cell that overruns should fail
   as that cell, not take the suite's clock with it, and results should print
   incrementally so a kill still leaves evidence.
2. **A cell whose image is absent must SKIP LOUDLY** (`nros_tests::skip!`),
   counted in the summary — never silently drop out of a green run. Whether
   tier 1 should then skip 15 of 16 is a lane question (RFC-0061), but it must
   be *visible* that it did.
3. Possibly split the embedded cells into their own binary so the toolchain-gated
   filters in `test-all` can exclude them by name, which they cannot today: the
   binary is `realtime_tiers_e2e` and the test is `realtime_tiers`, so no
   `~nuttx` / `~qemu` pattern matches, and the embedded cells ride into every
   tier-1 run.

## Not this

Not a flake, and not a stale fixture: the nuttx images were rebuilt from
scratch (`just nuttx build-fixtures-arm`, clean) and the timing and the failing
cell both reproduce exactly.
