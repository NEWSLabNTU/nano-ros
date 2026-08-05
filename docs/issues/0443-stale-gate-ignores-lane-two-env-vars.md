---
id: 443
title: "The staleness gate ignores the run's lane, because the lane is two env vars and `ci-matrix` sets only one"
status: open
type: bug
area: build
related: [issue-0196, issue-0368, issue-0439, phase-337]
---

## Symptom

`just ci-matrix` runs its staleness gate over the WHOLE tier-3 fixture set,
not over the lane — the opposite of what the recipe's own comment promises:

> Issue 0393 — this lane's BUILD is deliberately still `all`. … The tier-2
> saving is in the staleness GATE, which insists only the lane's coordinates
> are fresh.

There is no such saving today: `NROS_FIXTURE_SCOPE` is never set to `coords`, so
the gate takes its `all` default and demands freshness of every fixture in the
tree. Tier 2 pays tier 3's staleness cost while believing it does not.

**A caveat on how this was found**, because the first framing of this issue
overstated it. The trigger was a `missing` complaint for
`workspace-cpp-nuttx-riscv-realtime` (a `nuttx-riscv,cpp,zenoh` row, outside
tier 2's `nuttx-riscv,c,zenoh` coordinate) — but that fixture was absent because
the reporter had built `lane=tier2`, and this recipe expects an `all` build. Under
the intended workflow the row EXISTS and the gate's over-scoping shows up as
wasted work and spurious staleness, not as a missing file. The defect below is
real and independent of that mistake; the dramatic symptom was not.

## Root cause — one concept, two environment variables

The lane reaches the two gates by different names:

| consumer | reads |
|---|---|
| `_require-fixtures` (stamp coverage) | `NROS_FIXTURE_LANE` |
| `scripts/check-fixtures-stale.sh` (content freshness) | `NROS_FIXTURE_SCOPE` (+ `NROS_FIXTURE_COORDS`) |

`just ci` sets **both**:

```
NROS_FIXTURE_SCOPE=native NROS_TEST_SCOPE=native NROS_FIXTURE_LANE=native just check …
```

`just ci-matrix` sets **only the lane**:

```
NROS_FIXTURE_LANE=tier2 just check rust-rtos-link-check test-all
```

so `SCOPE` falls back to its `all` default and the staleness gate silently
audits the whole tier-3 fixture set while the run, the build and the stamp are
all scoped to tier 2. `ci-matrix-nightly` has the same omission.

Nothing detects the mismatch: `all` is a legitimate value, so the gate cannot
tell "the caller wants everything" from "the caller forgot the second variable".

## Why this is the issue-0196 class again

0196's rule is that build-side probes must watch the same inputs as test-side
gates. Here they watch different SETS entirely, and the divergence is expressed
as two spellings of one fact that a caller must remember to keep in sync — which
is the same shape as issue 0439 (two guards, each right alone, wrong together)
and as the `native`/`linux` vocabularies phase-337 W8.c had to move as one.

The mismatch is also silent in the dangerous direction. Today it over-demands,
which is merely obstructive. A future recipe that sets `SCOPE` narrower than
`LANE` would under-audit — reporting a lane green having freshness-checked less
than it ran, which is precisely the laundering the gate's own header warns
about.

**What this does NOT fix.** `ci-matrix` also leaves `NROS_TEST_SCOPE` unset, so
the RUN is not narrowed either — deliberately, per the 0393 comment: narrowing
the build "would need the run narrowed to match first". That is a separate,
declared design position, not part of this bug. The lane variable therefore has
THREE consumers (`LANE`, `SCOPE`, `TEST_SCOPE`) of which two should agree
automatically and the third is intentionally independent; this issue only
collapses the two that were meant to agree.

## Fix shape

Derive, do not duplicate. `_check-fixtures-stale` should compute `SCOPE` from
`NROS_FIXTURE_LANE` when `SCOPE` is not set explicitly:

* `LANE` unset / `all` → `SCOPE=all` (unchanged default)
* `LANE=native` → `SCOPE=native`
* any other lane → `SCOPE=coords` + `NROS_FIXTURE_COORDS=$(lane-coords <lane>)`,
  the same file `_lane-gate` and the build already use

An explicitly-set `SCOPE` keeps winning, so `just ci` and the per-lane
`check-fixtures-stale` recipe are untouched. Setting the LANE alone then means
one thing everywhere, and `ci-matrix` / `ci-matrix-nightly` need no second edit
— nor does the next lane recipe anyone adds.

## Related

- issue 0196 — build-side probes must watch the same inputs as test-side gates.
- issue 0368 F8 — the earlier instance of these two gates disagreeing about a
  lane, fixed for the stamp gate only.
- issue 0439 — sibling: two guards each correct alone, wrong in combination.
- phase-337 — its `just ci-matrix` acceptance criterion is blocked behind this.
