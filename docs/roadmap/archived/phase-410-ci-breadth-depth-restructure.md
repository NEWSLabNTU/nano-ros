# phase-410 — CI is BREADTH × DEPTH, and only depth is expensive

**Status (2026-09-01). W1–W4 landed. The phase is complete.**

Restructures the CI workflows so a tier's file keeps the promise the tier makes.
Consumes phase-411 (`just <verb> <scope>`) and phase-395 (the event design);
amends RFC-0061's tier ladder rather than replacing it.

## The defect this fixes

phase-411 gave every RFC-0061 tier a CI owner. That exposed the next question:
the owners are correct but the SHAPE is wrong, in three ways that only appear
under load.

**1. A tier's file is named for its EVENT, not its tier.** Tier 1 lives in
`host-tests.yml`, tier 2 in `post-submit.yml`, tier-2-nightly in `nightly.yml`,
tier 3 nowhere. "Where does tier 2 run?" needed a gate to answer.

**2. `gate.yml` does different work for five events in 850 lines.**
Deliberate (phase-395's economics) and unreadable: a developer cannot answer
"what runs on my PR?" without reading all of it. `nightly.yml` is another 851.

**3. The ladder collapses two independent axes.** This is the substantive one.

## BREADTH × DEPTH

| axis | values | cost |
| --- | --- | --- |
| **breadth** — which coordinates | tier1, tier2, nightly, full | low |
| **depth** — what we do with each | build+link, build+run | **high** |

Nearly all cost is DEPTH. Building wide is affordable; RUNNING wide is not.

RFC-0061's ladder encodes breadth only, so `just ci <tier>` always means
build AND run. The build-only depth exists — `just ci l3` is "cross build +
link", `rust-rtos-link-check` plus ELF symbol interrogation, no QEMU and no
tests — but it lives in the LANE vocabulary (L1/L3), a second ladder sharing
the `ci` namespace. `ci l3` and `ci full` read as siblings and are unrelated.

An earlier draft of this design proposed retiring the lane vocabulary into the
tiers. That was wrong and is recorded because the mistake is instructive: it
would have destroyed the build-only depth, which is the CHEAP half and the one
that can afford to be mandatory.

## The rule

> **Build+link is MANDATORY and WIDE. Build+run is SCHEDULED and NARROW.**

"It compiles and links for every target" is the regression that hurts most and
costs least to catch. Running is what cannot be afforded per merge.

## Why per-merge run-depth STARVES with ten agents

`post-submit.yml` sets `cancel-in-progress: true` — correct for "is main good
NOW", where a newer commit's answer subsumes an older one's.

Measured on this host, 2026-08-31: tier-2 fixtures rebuild in **11 min warm**
(~45 cold) and the tier-2 run takes **9.5 min** — 20+ minutes warm, before any
queueing.

With ten agents landing through a merge queue that batches up to four, merges
arrive faster than that. Each cancels the last, so **tier 2 completes never** —
and a lane that always cancels looks busy while reporting nothing, which is
strictly worse than the skipped job phase-411 just made visible.

So run-depth tiers move to a CLOCK, not to `push(main)`. A scheduled run always
finishes.

## Caching is a REPOSITORY-wide budget, not a per-job one

`gate.yml` already records why sccache is used over caching `target/`, and
the constraint that matters here: **the GitHub Actions Cache limit is 10 GB per
REPOSITORY**, and GitHub evicts entries "created and deleted at a high
frequency" (hence `SCCACHE_CACHE_SIZE=2G`).

Ten agents on ten concurrent PRs, each writing cache entries, thrash a shared
budget and make the cache worse than none.

**Rule: only `main`-branch runs WRITE the cache; PR runs READ it.** Any new
workflow must state which it is.

## Target structure

```
gate.yml          REQUIRED · check fast + test-unit
                  on: pull_request, merge_group           cheap; never starves

build-wide.yml    BUILD + LINK only, wide breadth         ← the mandatory promise
                  on: push(main)                            no fixtures, no QEMU

run-host.yml      build+run, host coordinates (tier 1)
                  on: push(main), schedule 03:00

run-matrix.yml    build+run, 1-wise (tier 2)
                  on: schedule 06:00, dispatch            ← clock, not per-merge

run-nightly.yml   build+run, pairwise      on: schedule 07:00
run-full.yml      build+run, everything    on: dispatch
```

Each file is phase-411 shaped: `just setup <scope>` then one tier command.
Filename ↔ tier ↔ local command, 1:1.

`cancel-in-progress: true` ONLY where a newer answer subsumes an older one AND
the run is short enough to finish — `gate`, `build-wide`. The scheduled `run-*`
files use `cancel-in-progress: false` so a run completes rather than being
perpetually superseded.

## THE MIGRATION CONSTRAINT — read before touching anything

The required status check is the job **named `CI`** (`ci-ok` in
`gate.yml`), and CLAUDE.md records that a required check producing no
verdict **deadlocked two pull requests**: a check that never reports blocks
forever rather than failing.

So: the aggregator must keep the literal name `CI` and must emit a verdict on
EVERY event, including when its dependencies skip. Its current `if: always()`
plus justified-skip logic moves VERBATIM; it is well built and this phase does
not improve it.

W1 lands that file ALONE and confirms the context still reports before anything
else moves.

## Work items

**W1 LANDED — the inference was right, and PROBING it found three things it
would have broken.**

The rename was deferred because the required status check is the context `CI`,
and CLAUDE.md records a required check with no verdict DEADLOCKING two pull
requests. The inference — that GitHub matches the JOB name, so a rename
preserves it — was strong but not observable from a shell: `gh pr checks`
reports no contexts for a branch even when a run has succeeded.

So it was tested rather than reasoned about. A throwaway branch, a real PR, and
a look:

```
CI     pass   4s
check  pass   8m42s
```

The context survives. Filename and workflow `name:` moved together, because a
partial rename leaves the workflow name matching and proves nothing.

**Three load-bearing references would have broken silently**, and none of them
is prose:

* the workflow's own `paths:` filter listed `.github/workflows/pr-checks.yml`,
  so after the rename it would have stopped triggering on edits to ITSELF;
* `check-ros-env-spelling.py` keys its allow-list on the path, so the renamed
  file would have lost its exemption and gone red for an unrelated reason;
* `queue-notify.yml` triggers on `workflow_run` with
  `workflows: ["pr-checks", "queue"]` — keyed on the workflow NAME, so the
  notifier would simply have stopped firing, with nothing to notice.

The third is the one that justifies the whole exercise: a `workflow_run`
trigger that no longer matches produces no error, no red, and no runs. It is
the same silent-skip class this phase and phase-407 spent their time on, and it
was one grep away from shipping.

**W2 — `build-wide.yml`.** The mandatory build+link lane on `push(main)`.

MEASURED 2026-08-31: `just ci l3` = **46 s**, exit 0 — `rust-rtos-link-check`
plus three cross ELFs interrogated and the heap-free image link-gated, without
booting anything. Against 20+ minutes for run-depth, build depth is roughly
**25x cheaper**, so per-merge is comfortable and the starvation argument does
not apply to it.

Caveat, stated because it changes the number and not the conclusion: 46 s is
WARM, on a tree where the cross artifacts already exist. A cold runner pays the
cross builds themselves, which is the bulk. The RATIO is what the design rests
on, and no plausible cold cost closes a 25x gap.

**W3 LANDED (tier 2). Tier 1 and nightly stay where they are, deliberately.**

Tier 2 moved out of `post-submit.yml` into `run-matrix.yml` on a 06:00 cron with
`cancel-in-progress: false`. That is the starvation fix and the measured case.
`dep-chain` stays in post-submit: hosted, 158 s, genuinely per-merge.

Tier 1 (`host-tests.yml`) keeps `push(main)` — it is the cheapest run depth, it
is path-filtered, and that file has NO concurrency group, so runs QUEUE rather
than cancel. Queueing wastes runner time under ten agents but it does not
starve, and changing it is a separate judgement about hosted-runner budget
rather than about correctness. Recorded here so the inconsistency is a decision
and not an oversight.

Tier-2-nightly is already on a clock by construction.

**W4 LANDED — depth is a dimension, and "lane" was overloaded three ways.**

The word named three things in one repo, meaning two:

| name | where | means |
| --- | --- | --- |
| `CiTier` | `buckets.rs` | the ladder rung (4, incl. Tier3) |
| `CiLane` | `ci_lane.rs` | the COMPUTED cell selection for a rung (3) |
| `_NROS_LANES` | `fixture-lane.sh` | the fixture coordinate set — breadth |
| `ci l1` / `ci l3` | `just/ci.just` | DEPTH — compile+unit / cross build+link |

Tier-vs-lane is a real distinction (rung vs its computed selection) and is left
alone: `CiLane` (71 refs) and `nros_lane_*` (222 refs) are BREADTH and this work
does not touch them. The collision was narrow — `l1`/`l3` used "l" for depth.

**`l1` is not a rung and never was.** It visits no coordinates: `check` +
`test-unit`, no fixture, no platform, no QEMU. It is now `just ci gate`, with
`l1` kept as a forwarder. Forcing it into (breadth, depth) is the error an
earlier draft made.

**`l3` becomes a depth on a breadth**: `just ci matrix build`. `l3` survives as
the implementation.

**MEASURED, and it changed the design twice:**

* `just ci matrix depth=build` does NOT work. `just` does not parse `name=value`
  for a MODULE recipe — it yields the literal `depth=build`. Positional
  (`just ci matrix build`) does. The named-argument design was abandoned on this
  measurement, not on taste.
* `just ci matrix build` = **27 s** (`ci l3` measured 46 s earlier on a colder
  tree). Build depth stays comfortably per-merge; `build-wide.yml` now names the
  pair rather than a lane letter.

**Each depth dispatches to a FIXED inner recipe** (`_matrix-run`,
`_matrix-build`) rather than branching inside one parameterised body, because
`check-lane-contracts` proves affordability by WALKING a recipe body and cannot
verify a conditional one.

**And that gate could not see private recipes at all.** Its `RECIPE` pattern
required `^[a-z]`, so every `_`-prefixed recipe was invisible — including
`_lane-gate`, which `ci matrix` calls. The closure silently stopped at each
private boundary, which is the vacuous-pass shape the file's own header warns
about. Widened; the larger closure still reports clean, now across 3
affordability tiers instead of 2.

## Acceptance

* every tier's file name names its tier, and its body is a command a developer
  can type;
* build+link is mandatory per merge; no run-depth lane is per-merge;
* exactly one cache writer;
* the `CI` context never stops reporting, on any event;
* `check-tier-has-ci-owner` still passes, and its OWNERLESS list does not grow.

## The open number, now closed

`just ci l3` = **46 s warm** (exit 0). W2 is per-merge.

Still unmeasured, and W2 should report it once the lane exists: the same
command on a COLD runner. It cannot change the structure — a 25x gap does not
close — but it decides whether `build-wide` needs the sccache read path to be
fast or merely present.
