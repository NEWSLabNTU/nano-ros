# Phase 559 — CI workflow organization: 21 files to 13, each split named by a constraint

**Status (2026-11-13). OPEN: the layout is ruled, no migration item has
landed.** User ruling 2026-11-13, verbatim: "Change weekly.yml to run on
Sundays. The overall organization is good to me. Update phase docs and
implement it." This phase is the record for how `.github/workflows/` is laid
out. [RFC-0122](../design/0122-behaviour-bringups-road-witnesses-cadence-backed-ci.md)
D8/D9 (which job keeps which cadence) and
[phase-535](phase-535-example-reorg-and-cadence-ci.md) W8 point here for the
file each job lives in. Owners: nros 175 (coordinator of this phase), nros 118,
nros 142, wtf, as named per item.

Each item is one small PR, claimed with `just claim phase-559-wN` before it
starts. Every PR that moves or renames a workflow or a job updates, in the
same PR, every consumer that reads the name. The known readers:
`record-lane-runs`, `check-cadence-jobs` (`WEEKLY_WF`, the daily binding),
`tier-health`, `check-lane-stage-reporting`, `nightly-triage`,
`check-default-gates-run-somewhere`, `check-merge-group-gates`,
`check-tier-has-ci-owner`, `check-board-registry`, `check-book-probe-tracks`,
`gen-support-status`, `gen-board-support`, `next-stable-delta.py`,
`check-workflow-job-timeouts`, `check-workflow-runner-isolation`,
`check-workflow-just-provisioning`, and the workflow-name lists in
`queue-notify.yml`, `nightly-report.yml` and `pr-verdicts.yml`. A PR finds the
rest with `git grep -n '<old file or job name>'`.

## Why

21 workflow files grew one lane at a time (phase-253 merged 13 into 6 tier
files; they split again). The cost is not the count. It is that a file
boundary is a fact several gates and triage tools key on, and nobody can say
why a given boundary exists. Measured on 2026-11-13: 35 of the jobs have no
`timeout-minutes`; `nightly.yml` carries three crons (05:00, 07:00, a weekly
09:00) behind `github.event.schedule` guards; the book's bootstrap/installed
probes run in both `nightly.yml` and `probe.yml`; `gate.yml` is both the
required gate and a nightly (`schedule` 02:00, `check-build`).

The rule this phase applies: **a file boundary exists only where a named
constraint forces it.** Everything else that shares a cadence and a runner
class shares a file.

## Constraints

HARD (GitHub forces them):

- **H1** `needs:` cannot cross files. Everything the required `CI` judges
  lives in `gate.yml`.
- **H2** Required checks match by NAME. `CI` stays a top-level job, never in a
  reusable workflow, never path-filtered (a check that never reports blocks
  forever; two PRs deadlocked on it, 2026-08-28).
- **H3** `workflow_run` fires per WHOLE run, so a consumer of one lane's result
  needs that lane in its own file.
- **H4** A `workflow_call` callee is its own file.
- **H5** Path filters are per file and trigger.
- **H6** Privileged plumbing (`pull_request_target`, `workflow_run` with a
  write token) never shares a file with jobs that run PR code.

SOFT (chosen, each for a tool or an operator):

- **S1** One cron per file wherever a tool reads "the latest scheduled run"
  (`nightly-triage`, `tier-health`).
- **S2** One run conclusion per file, so a red run names one lane.
- **S3** One `workflow_dispatch` input form per file.
- **S4** Disabling is per file (`gh workflow disable`), so things that must be
  switched off independently live apart.

## Target layout (13 files)

| # | file | holds | split forced by |
| --- | --- | --- | --- |
| 1 | `gate.yml` | `CI` and every job it judges, plus cache warmers. NO `schedule`. | H1, H2 |
| 2 | `queue.yml` | advisory L3 on the dedicated label | S2 (advisory, never `CI`) |
| 3 | `post-submit.yml` | unchanged | S2 |
| 4 | `nightly.yml` | ONE daily cron, board verdicts only: per-board platform jobs, tier-1 hosted per-board shards, Zephyr 2 lines × 3 languages, px4-sitl, nuttx-make-road, Zephyr template copy-out | S1 |
| 5 | `run-matrix.yml` | self-hosted tier 2, once a day (gains `matrix-nightly`) | S1, runner class |
| 6 | `host-tests.yml` | unit / ignored / doc, feature-sweep, rt-sched, `check-build`, lane-census, the book probe tracks, the Humble + Jazzy edition lane | S1 |
| 7 | `live-peer.yml` | unchanged | S4 |
| 8 | `weekly.yml` | **Sunday** cron: probe-platforms, zenoh-probe, next-stable, verify-verus | S1, S4 |
| 9 | `release-nros.yml` | release, plus the release check (`just ci full` before publishing) | S3 |
| 10 | `starter-image.yml` | unchanged | H4 |
| 11 | `publish.yml` | docs + images | S2 |
| 12 | `automation.yml` | arm-auto-merge, queue-notify, nightly-report, pr-verdicts, roadmap-audit | H6 (apart from PR code) |
| 13 | `merge-runtime.yml` | dispatch-only re-measure tool | S3 |

Composite actions: `setup-nros-cli` (the ONLY road that builds the CLI in
CI), `provision-gate-lane`, `report-lane-stage`.

**Provisional defaults** (open questions, taken as below until the user
rules; flagged to the user):

- L3 stays advisory.
- The release check lives in `release-nros.yml`.
- The book probe tracks live in `host-tests.yml`.
- Tier 2 keeps its 1-wise cover.

## Work items

### W1 — A timeout on every job; the timeout gate reads every file

- **Owner:** nros 118.
- 35 jobs have no `timeout-minutes` (2026-11-13 inventory). Each gets one
  from its measured longest successful run, doubled (the gate.yml rule).
  `check-workflow-job-timeouts` widens from its current scope to every file.
- **Acceptance:** the gate passes over all workflow files, and its selftest
  refuses a job with no timeout in a file it did not read before.
- **Status:** OPEN

### W2 — Composite actions

- **Owner:** nros 118.
- `setup-nros-cli` becomes the only CLI road (no inline `cargo build` of the
  CLI in any workflow), and `provision-gate-lane` and `report-lane-stage` are
  extracted.
- **Acceptance:** a gate refuses an inline CLI build in a workflow.
- **Status:** OPEN

### W3 — The queue's L3 on its dedicated label

- **Owner:** nros 118 (in flight).
- **Status:** OPEN — in flight (nros 118).

### W4 — `nightly.yml` has one cron; `weekly.yml` runs on Sunday

- **Owner:** nros 175.
- `nightly.yml` keeps ONE daily cron. Its weekly `zenoh-probe` (09:00 Monday)
  moves to `weekly.yml`. `bootstrap-probe` and `installed-probe` are removed
  as duplicates of `probe.yml`'s tracks (W6 homes those).
- `weekly.yml` is new, on ONE Sunday cron (user ruling), and absorbs
  `probe-platforms.yml` and `next-stable.yml` (both deleted), plus the Verus
  proofs (issue 2499's lane lands as a `weekly.yml` job, not a file of its
  own).
- The Zephyr 05:00 and platform 07:00 blocks share the one daily cron.
  `github.event.schedule` guards go. This half (W4b) rebases onto phase-558's
  #3715 (merged), which rewrote the 07:00 platform matrix and the board job
  names.
- Consumers updated in the same PR: `check-cadence-jobs` (`WEEKLY_WF`),
  `check-board-registry`, `check-book-probe-tracks`, `gen-board-support`,
  `next-stable-delta.py`, `nightly-triage`, and the `nightly-report` name list.
- **Acceptance:** `nightly.yml` and `weekly.yml` each have exactly one
  `schedule` entry, and `weekly.yml`'s is a Sunday. No two files duplicate a
  probe track. Every gate that reads a moved name passes and was changed in
  the same PR.
- **Status:** OPEN — W4a (`weekly.yml` on Sunday, the duplicate probes)
  done in #3713 (2026-11-13). W4b (one daily cron) is unblocked: #3715 merged.

### Per-board platform jobs — tracked in phase-558, not here

- **Status:** in [phase-558](phase-558-nightly-per-board-verdicts.md) (#3715, merged 2026-11-13; nros 142). One
  platform job per board in `nightly.yml`'s 07:00
  matrix, instead of one per platform family. Its work items, acceptance and
  status live there only; this phase depends on it (W4b, W5) and does not
  restate it.

### W5 — `matrix-nightly` to `run-matrix.yml`; tier 1 to nightly per-board shards

- **Owner:** nros 175.
- **W5a.** `nightly.yml`'s `matrix-nightly` and `coverage-matrix-nightly`
  move to `run-matrix.yml` (self-hosted, one daily cron), chained after
  `tier1` on the single-occupancy runner.
- **W5b.** `run-matrix.yml`'s `tier1` runs on the SELF-HOSTED runner, not a
  hosted one. It left the hosted runner because the full tier-1 fixture build
  did not fit (issues 1684/1685: +37 G for the linux half alone against ~16 G
  free, and `just setup linux` does not provision Zephyr native_sim or
  ThreadX-Linux). So tier 1 moves to hosted per-board shards in `nightly.yml`
  ONLY IF a measurement says each shard fits. First a dispatch-only job runs
  one shard per host witness board (linux, zephyr native_sim, threadx-linux)
  under phase-558's per-board narrowing, and records disk and time. If the
  shards fit, tier 1 moves; if not, the numbers go to the user, because
  RFC-0122's "tier 1, daily, hosted shards" would then be a D9 lowering
  (user's call, coordinator nros 241 manager).
- **Acceptance:** W5a: `check-tier-has-ci-owner` passes with the new owner,
  and `matrix-nightly` runs once a day. W5b: the measurement is recorded
  here; if tier 1 moves, a host witness row's daily binding names the nightly
  per-board shard and `check-cadence-jobs` passes.
- **Status:** BLOCKED (issue 2475) — W5a DONE (#3727, 2026-11-13).
  W5b's tool is in `merge-runtime.yml` (`lane: tier1`). Its first
  measurement, run 43390508641 (2026-11-13, hosted `ubuntu-22.04`, one shard
  per host-witness board), reached NO fixture build. All three shards failed
  on issue 2475's provisioning defects, the ones that already stop the merge
  lane:

  | shard | time | setup | `just build tier1` | failed on |
  | --- | --- | --- | --- | --- |
  | linux | 58.8 min | ~6.4 min | 51.9 min | `check fast`: `python-entry-provenance` NOT VERIFIED (no `rosidl` clone) |
  | threadx_linux | 61.8 min | ~6.4 min | 54.4 min | `check fast` 551/551, then `_require-build-sources`: micro-xrce-dds-client, micro-cdr, px4-autopilot |
  | zephyr | 53.3 min | ~10.6 min | 42.2 min | lane preflight: no pinned cargo-nextest in the zephyr-ci image |

  What it does say: free disk at the start of the build was 74-76 G (the
  `nano-ros-ci` shards) and 50 G (zephyr-ci), after `reclaim-disk.sh`, far
  more than the ~16 G issue 1684 measured. And `just build <lane>` spends
  42-54 min in tree-wide codegen and `check fast` before its first fixture
  (issue 2475 defect 4). So the disk question is still open, and the time
  question already looks bad for a daily hosted tier 1 unless defect 4 is
  fixed. W5b re-measures once issue 2475 lands.

  Order (2026-11-13, coordinator): issue 2475 defects 1-3 first (owner
  nros 175: provisioning in `merge-runtime.yml`, nextest baked into the
  zephyr-ci image); then the W5b re-measure, numbers reported even if only
  the time is bad; then 2475 defect 4 scoped as a phase-535 work item; W7
  last and alone.

### W6 — `host-tests.yml` absorbs `check-build`, the census and the book probes

- **Owner:** nros 175; the Humble + Jazzy edition lane is wtf's.
- `gate.yml`'s scheduled `check-build`, `lane-census.yml` (deleted), and
  `probe.yml`'s three tracks (deleted) join `host-tests.yml`'s one daily cron.
- **Acceptance:** `lane-census.yml` and `probe.yml` are gone, and each of their
  jobs runs in `host-tests.yml` under the same `just` verb. `CI` no longer
  lists `check-build`, which moves off `gate.yml` in W7.
- **Status:** OPEN

### W7 — `gate.yml` drops its schedule; the Zephyr copy-out goes to nightly

- **Owner:** nros 175.
- The `CI` aggregator change, ALONE in its own PR, verified on a merge-group
  dry run.
- `gate.yml` loses `schedule`. `check-build` leaves for `host-tests.yml` (W6).
  `check-template-copy-out-zephyr` (on `gate.yml`'s schedule since #3710, user
  ruling "Do Zephyr template check daily") moves to `nightly.yml`.
- `check-merge-group-gates`' `DEFERRED_PARTS` then names `nightly.yml`, not a
  `CI` sibling, as where the zephyr part is heard. A deferred part outside
  `gate.yml` cannot be a `CI` sibling (H1), so the rule changes from "heard by
  `CI` on schedule" to "run by a named job on a daily schedule".
- **Acceptance:** a merge-group run of the PR is green, with `CI` listing no
  job that no longer exists. `check-default-gates-run-somewhere` and
  `check-merge-group-gates` pass and refuse the old placement.
- **Status:** OPEN

### W8 — `publish.yml`, then `automation.yml`

- **Owner:** nros 118.
- `docs.yml` and `images.yml` merge into `publish.yml`. The five plumbing
  workflows merge into `automation.yml` (H6: none of them runs PR code).
- **Acceptance:** 13 files. Every `workflow_run` consumer names the new
  workflow names.
- **Status:** OPEN

## Acceptance (phase)

- 13 workflow files, as in the table. Every split is named in the table by a
  constraint.
- Every job has a `timeout-minutes`, and the gate reads every file.
- `nightly.yml`, `weekly.yml`, `host-tests.yml` and `run-matrix.yml` each
  carry ONE cron. `weekly.yml`'s is a Sunday.
- No job runs in two files.
- `docs/development/ci-workflow-reorg.md` (the phase-253 plan) is replaced by
  the table above, or points here.

## Coverage note: what the merge queue builds for Zephyr (2026-11-13)

[phase-410](archived/phase-410-ci-breadth-depth-restructure.md) states "Build+link
is MANDATORY and WIDE. Build+run is SCHEDULED and NARROW." What merges get
today is narrower than that, for Zephyr in particular:

- **Per merge group.** `queue.yml`'s L3 (`just ci matrix build`), measured by
  nros 243 on run 43385631926, links 3 cross ELFs: the FreeRTOS, NuttX and
  ThreadX-Linux Rust talkers. Zephyr leaves only go through `nros sync`; no
  Zephyr image is built or linked. The one per-merge Zephyr BUILD was
  `gate.yml`'s `check-template-copy-out-zephyr`, and #3710 moved it to the
  daily schedule (user ruling "Do Zephyr template check daily"). So since
  #3710, **no per-merge lane builds or links a Zephyr image**.
- **Daily.** `nightly.yml`'s Zephyr jobs build and link Zephyr images every
  day: `zephyr-example-matrix` runs `just zephyr build-one` (a west build to an
  ELF) for 4 examples (c/talker, cpp/talker, rust/talker, rust/action-server)
  on both lines (3.7, 4.4), `zephyr-dual-line-summary` runs `ci-both`, and
  `zephyr-copy-out` plus `gate.yml`'s daily `template-copy-out [zephyr]` build
  copied-out projects. A Zephyr link regression is caught within a day.
- **phase-535 W8 item 4 (2 lines × 3 languages with native_sim runtime) does
  not change the link answer.** Daily link coverage already exists. W8.4 adds
  RUNTIME (native_sim cells) and reshapes the matrix; it closes issue 2462
  ("Zephyr daily cadence has no runtime job"), not a link gap.

Per-merge Zephyr link coverage, if wanted, is a question for the user (it
would cost a Zephyr SDK + west workspace on the queue's critical path, which is
what #3710 just took off it). Not added here.

## Issues homed here

- [issue 2502](../issues/2502-zephyr-template-copy-out-doubled.md) — the
  Zephyr template copy-out job nearly doubled in a week (filed with #3711).
  W7 moves the job, and the slowdown goes with it.
- [issue 2499](../issues/archived/2499-verus-lane-toolchain-vstd-mismatch.md) — the
  Verus lane, resolved by #3723: its job is `verify-verus` in `weekly.yml`.
