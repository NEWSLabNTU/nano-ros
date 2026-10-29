---
id: 2090
title: "check-artifact-identity-budget's self-test case (c) failed once on main and passed on the next push with no change"
status: open
type: bug
area: [ci, testing]
severity: low
found: 2026-10-26
related: [0499, 0513, 0661]
---

## Measured

`gate` push run 40313343797 on main `3cd1d6ba9` (2026-10-26 11:12 UTC), job
121539695512 ("warm the compile-tier cache (main only)"), step `check fast`:

```
===== FAIL (artifact-identity-budget, rc=1, 3974ms) =====
  FAIL (c): filtered on the sidecar without saying the build was partial
check-artifact-identity-budget --self-test: 1 case(s) FAILED
check-fast (parallel): 1 of 486 gate(s) FAILED
```

The required `CI` job (121547864875) on that run is red as a result.

The next push `gate` on main (`7de7e1f86`, run 40318889495) and every one after
it through `8de84cf0c` passed. None of the three commits between
`3cd1d6ba9` and `7de7e1f86` touches `scripts/check-artifact-identity-budget.sh`
or `scripts/lib/`, and `3cd1d6ba9` itself (phase-516 W1a/W1b, the router
downsampling rule) does not either. So the same self-test, on the same script,
gave two answers.

## What it is not

- Not a defect in `3cd1d6ba9`: the change is router-rule emission and `just`
  recipe edits, with no path into the identity-budget gate.
- Not a known cause: it is not the disk class (issue 1353; the log has no
  `No space left`) and the gate printed a verdict, so the runner did not die.

## Where to look

Case (c) writes a `<stamp>.started` sidecar with no stamp, back-dates the
rlibs with `touch -d`, re-invokes the script, and requires the output to say
`PARTIAL build`. The failing branch means the bound WAS used (no
`NO started_at`) but the wording did not appear. The log keeps only the
verdict line, not the script's output for the case, so which reading it took
is not recoverable from this run. Two candidates worth ruling out first:
`check-fast` runs gates in parallel, so anything else in the lane that writes
or clears the real fixture stamp/sidecar while case (c) reads its own; and the
self-test's temp tree being shared rather than per invocation.

## What would close it

Make case (c) print the script's output when it fails, so the next occurrence
names its own reading; then either reproduce it under `check-fast`'s
parallelism and fix the shared state, or show it cannot recur.

## Second sighting (2026-10-29)

Push gate run **40889496628** on main `d8daf0330`, job **123502240409**
`check / fast (every event)`, on a GitHub-hosted runner:

> FAIL (c): filtered on the sidecar without saying the build was partial
>
> check-fast (parallel): 1 of 491 gate(s) FAILED

The same failing branch as the first sighting. It passed in five consecutive
local runs on the same commit. The merge-queue batch that produced that tree
(#2832's) passed it too, and re-running the failed job (attempt 2,
job 123525046968) passed. Nothing changed between the red and the green.

Two sightings in four days, both on hosted runners under `check-fast`'s
parallel scheduler, both on the second branch.

Done here: item 1 of "What would close it". Case (c) now prints the script's
output, prefixed `(c) |`, on either failure branch, so the next occurrence
records which reading it took.
