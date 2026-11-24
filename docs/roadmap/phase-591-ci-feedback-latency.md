# Phase 591 -- CI feedback latency: a merged fix is proven in hours, not days

**Status (2026-11-24). OPEN: plan approved by the user; W1-W8 assigned, none landed yet.
W2's evidence rule is decided (a green dispatch on `main` at the release commit counts) and
is being applied by the phase-590 owner.**

## Why

During the nros12 blocker push (phase-590, 2026-11-22..24) most fixes sat merged and unproven
for a day or more, and agents could not move on. Three causes, all measured:

1. **The scheduled lanes rarely produce a verdict.** Over the last 30 runs of each:

   | lane | failed | median | max |
   | --- | --- | --- | --- |
   | `nightly` | 29 of 30 | 127 min | 657 min |
   | `run-matrix` (tier 1 + 2) | 27 of 30 | 269 min | 1009 min |
   | `live-peer` | 21 of 30 | 182 min | 389 min |
   | `host-tests` | 15 of 30 | 113 min | 200 min |

   Most reds stop in the fixture BUILD, before any cell runs, so each blocker hid the next
   one for a night: B11, then B12/B13/B14, then B19, then B22.
2. **Only a scheduled run counted as release evidence,** and each lane runs once a day. A fix
   that merged at noon was proven the next morning at the earliest.
3. **Local full-gate capacity.** A full `just ci gate` takes 60-110 min on host 241
   (about 25 min on 243), each host runs one at a time, 241 had a 03:00-12:10 UTC no-gate
   window, 133 was below the 150 GB disk floor and 118 runs no gates. On 2026-11-23, 241 alone
   had five or six gates queued, and non-blocker PRs slipped a day.

The fast path is healthy and is not in scope: `pr-checks` median 8 min, the required `gate`
21 min (max 40), merge-queue runs 22 min.

## Targets

- Every scheduled lane produces a verdict on every run (cells ran), even when a fixture
  build fails.
- A merged lane fix is proven within about 6 hours.
- Tier 2 wall time under 5 hours; the `live-peer` host job under 120 min (under half its
  350-min cap).

## Work items

### W1 -- keep-going fixture builds (owner: nros 118)

- **Status:** OPEN

Today a lane's build stage stops at the first failing fixture row, so no cell runs. Change the
lane build steps (`build-test-fixtures` and the per-lane build recipes) to build every other
row, run every cell whose fixtures exist, list every failed row at the end and still exit
red. A missing fixture must stay a hard failure for its own cells, never a silent skip
(the no-vacuous-tests rule).

Acceptance: a planted failing row leaves every other row built and its cells run; the run's
summary names each failed row; the lane is still red.

### W2 -- evidence from a dispatch, and automatic re-runs (owners: nros 243 rule, nros 118 automation)

- **Status:** OPEN

User ruling 2026-11-24: a green `workflow_dispatch` of a scheduled workflow, on `main` at the
release commit, with every job green, counts as release evidence like a scheduled run.
Applied in `release-lanes-green.py` and phase-590 by the phase-590 owner.

Then: when a pull request that states which red scheduled lane it fixes merges, dispatch that
lane on `main` automatically, so nobody has to remember to.

Acceptance: the release check accepts a qualifying dispatch and refuses one with a skipped
job or at another commit (self-tests with a negative control); the next lane fix after this
lands is proven within 6 hours of its merge.

### W3 -- `live-peer` host job time (owner: nros 118)

- **Status:** OPEN

The host job took 342 of its 350 min on 2026-11-23 (fixture step 5 h 12 min): about 140
fixture rows, each rebuilding the core chain from scratch, and a configure prepass that
re-configured every image in a workspace per row. The row narrowing and the image-scoped
prepass are on `main` (`fe98960b22`, phase-590 B16). Remaining: share a build directory per
coordinate across rows, so the core chain compiles a handful of times, not about 130.

Acceptance: the host job under 120 min on two consecutive scheduled runs.

### W4 -- tier 2 in parallel (owner: nros 118)

- **Status:** OPEN

`run-matrix` runs tier 2 (269 min) and the tier-2 pairwise cover (418 min) back to back on one
self-hosted runner, so a run takes 11-17 h. Run them on separate runners in parallel. The
"Sweep orphans and disk" step took 60 min in run 44030238880; find out why and shrink it.

Acceptance: tier 2 wall time under 5 h; the sweep step under 10 min.

### W5 -- nightly platform build time (owner: nros 241)

- **Status:** OPEN

Each hosted nightly platform job spends 103-109 min building and 63-70 min testing
(ThreadxLinux, FreertosMps2, run 45115997343). Reuse build output across runs (a build cache
keyed on the lockfiles and toolchain) or share build directories as in W3; moving a platform to
a self-hosted runner is the fallback, weighed against W7 since it takes gate capacity.

Acceptance: each nightly platform job builds in under 60 min.

### W6 -- `host-tests` shard balance (owner: nros 142, taken over from nros 118 on 2026-11-24)

- **Status:** OPEN -- in review as #4754: six shards split by measured weights
  (`scripts/ci/build-gate-weights.py`), the last shard charged for its
  `no-std` + `pristine-checkout`. Measured on branch dispatches: three balanced
  shards ~85 min each; four 73 / 76 / 91 / 63 min (run 45319129706); five
  61 / 79 / 53 / 67 / 53 min (run 45331627283), the 79 on an EPYC 7763.
  **The CPU a shard draws decides its time, not the weights:** a 7763 runs
  these gates ~1.5x slower than an EPYC 9V45 (nros 118, eight scheduled runs),
  and both weight sets (six-run means, eight-run medians) predicted the five
  shards flat at ~60 min of test time against 78 / 77 / 68 / 59 / 34 measured.
  Six shards give ~60 min on a 7763. Acceptance is the first scheduled run
  after merge (no further branch dispatch, the nros 241 manager's call).

The three build-tier shards take 102 / 68 / 68 min (run 45104661214). Rebalance the rows so no
shard exceeds 80 min.

Acceptance: each shard under 80 min on a scheduled run.

### W7 -- local gate capacity (owners: nros 133 disk, nros 241 diagnosis)

- **Status:** OPEN

User ruling 2026-11-24: 241's no-gate window shrinks to 04:30-08:00 UTC (no full gate STARTS
then), and host 133's disk is to be freed for gates. Remaining: 133 back above 150 GB (its own
build output first; anything of other tenants goes to the user), and 241 measures why its gate
takes 60-110 min against about 25 min on 243 (load average about 75 on a shared host).
Skipping parts of the gate is out of scope, per the standing rule.

Acceptance: three hosts can run a full gate; no release-blocker gate waits more than 4 h for
a slot.

### W8 -- lane health watch (owner: nros 142, taken over from nros 118 on 2026-11-24)

- **Status:** OPEN -- in review as #4754: `scripts/ci/lane-health.py` (run daily
  by `automation.yml`'s triage job) names a scheduled lane `NO VERDICT 2d`
  when neither of its last two UTC days had a run whose cells ran. Replayed
  over 2026-11-18..23 it names `run-matrix` on every day from 2026-11-19.

A daily report that flags any scheduled lane with no verdict for two consecutive days, read in
the morning triage, so a lane cannot sit red-without-a-verdict for a week again.

Acceptance: the report runs daily and names such a lane.

## Order

W1, W2 and W3 first (the largest gains; W2's rule and W3's first half already landed). Then W4,
W6 and W8 in the same week. W5 next, after its build-cache or self-hosted decision. W7 ongoing.

## Risks

- W1 must not turn a missing fixture into a pass: every failed row stays a named failure.
- W5 on self-hosted runners would take gate capacity from W7; decide W5 after W7's numbers.
