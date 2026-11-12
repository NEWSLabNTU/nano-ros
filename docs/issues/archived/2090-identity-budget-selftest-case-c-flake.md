---
id: 2090
title: "check-artifact-identity-budget's self-test case (c) failed once on main and passed on the next push with no change"
status: resolved
resolved: 2026-11-12
resolved_in: "every status-read pipeline into an early-exit matcher converted to a here-string; the sigpipe gate's allowlist deleted"
type: bug
area: [ci, testing]
severity: low
found: 2026-10-26
related: [phase-466, 0499, 0513, 0661, 1077, 0726]
---

**Home: [phase-466](../../roadmap/phase-466-scheduled-lanes-produce-verdicts.md)** — homed 2026-11-04: a self-test that gives two answers on the same script is a lane verdict nobody can trust.

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

## Third sighting, and the cause (2026-11-12)

Push gate run **41568610057** on main `442aaf2379` (2026-11-03), step
`check fast`. This time case (c) printed what it judged:

```
(c) | artifact-identity budget: FAIL
(c) |   nros_core was NOT rebuilt since started_at=2010-01-01T00:00:00Z (an incremental
(c) |   build with nothing to do for it), so this counts ALL 6 rlib(s) in the
(c) |   tree — an accumulated tree can inflate it (issue 0499).
…
(c) |   tree-wide axes read the 6 rlib(s) THIS build wrote (2 crate(s)); …
```

The gate took its issue-0499 widening, "the window holds rlibs but none for
`nros_core`", while its own last line says all 6 rlibs, including the five
`nros_core` ones, were inside the window. That widening note carries no
`$_started_src`, so the "PARTIAL build" wording case (c) asserts was missing.

The widening's test was

```sh
if [ -n "$rlibs" ] \
    && ! printf '%s\n' "$rlibs" | grep -q "/lib${BUDGET_CRATE}-" \
    && printf '%s\n' "$_all" | grep -q "/lib${BUDGET_CRATE}-"; then
```

in a script that sets `pipefail`. This is issue 1077's race: `grep -q` exits
at the first match, bash's builtin `printf` is still writing the later lines,
takes SIGPIPE, and `pipefail` makes the pipeline's status 141. So
`! printf … | grep -q` read a MATCH as a miss, and only under load, which is
why every sighting was on a hosted runner under `check-fast`'s parallel
scheduler and none reproduced locally (400 self-test runs at 48-way here: 0).

`check-pipefail-sigpipe-assertions` (issue 1077) exists for exactly this
shape, and had these two lines on its ALLOWLIST, a ratchet of 30 pre-existing
sites whose stated premise was that they "sit outside the `check-fast` lane".
This one does not: the gate's self-test runs in `check-fast`.

**Fix.** Every one of the 30 allowlisted sites, in 22 files, now captures and
matches a here-string (`grep -q X <<<"$(cmd)"`, `grep -q X <<<"$var"`, or
`[ -n "$(find … -print -quit)" ]`), and the allowlist is deleted from the
gate, together with its stale-entry check. A ratchet is only as true as the
claim that each entry is harmless, and nothing re-checks that claim. Most of
the other sites carry the same hazard on the fixture-build path: a
`make --version | head -1 | grep -q 4.4` kills `make` with SIGPIPE when
`head` exits, and `pipefail` then reports the pinned make 4.4 as absent.
`scripts/grep-q-baseline.json` loses the one `grep -q` site the conversion
removed.

Sweep: `python3 scripts/check-pipefail-sigpipe-assertions.py` (no allowlist,
so it is the whole sweep).

