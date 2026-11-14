# Phase 559 — CI workflow organization: 21 files to 13, each split named by a constraint

**Status (2026-11-13). OPEN: W1, W2 and W4 are DONE; W3 and W6 have
landed, each with a named remainder; W5b waits on issue 2475; W7 and W8
have not started.** User ruling 2026-11-13, verbatim: "Change weekly.yml to run on
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
`check-workflow-just-provisioning`, and the `workflow_run` workflow-name list
in `automation.yml` (its `enqueue`, `notify` and `triage` jobs). A PR finds the
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

**Defaults, confirmed (user, 2026-11-13).** These were provisional until
the user ruled; all five stand:

- L3 stays advisory.
- The release check lives in `release-nros.yml`.
- The book probe tracks live in `host-tests.yml`.
- Tier 2 keeps its 1-wise cover.
- `merge-runtime.yml` stays as the dispatch-only re-measure tool.

## Work items

### W1 — A timeout on every job; the timeout gate reads every file

- **Owner:** nros 118.
- 35 jobs have no `timeout-minutes` (2026-11-13 inventory). Each gets one
  from its measured longest successful run, doubled (the gate.yml rule).
  `check-workflow-job-timeouts` widens from its current scope to every file.
- **Acceptance:** the gate passes over all workflow files, and its selftest
  refuses a job with no timeout in a file it did not read before.
- **Status:** DONE (#3719, 2026-11-13). Every job in every workflow states
  a measured timeout; `check-workflow-job-timeouts` reads every file, with
  self-test cases for the hosted (355) and self-hosted (720) ceilings. Two
  values are provisional and say so in place: `matrix-nightly` (sized from a
  failed run, re-measure after its first green) and starter-image `retag`
  (never run).

### W2 — Composite actions

- **Owner:** nros 118.
- `setup-nros-cli` becomes the only CLI road (no inline `cargo build` of the
  CLI in any workflow), and `provision-gate-lane` and `report-lane-stage` are
  extracted.
- **Acceptance:** a gate refuses an inline CLI build in a workflow.
- **Status:** DONE (2026-11-13). `provision-gate-lane` and
  `report-lane-stage` are composite actions (#3725). `check-ci-cli-from-source`
  arm C refuses a CLI build spelled in any workflow's own `run:` text, with or
  without a stamp assertion: `cargo build|install|run` selecting the `nros`
  binary or the `packages/cli` workspace, `just setup-cli`, and
  `scripts/bootstrap.sh`. Only `setup-nros-cli` may spell it, and the gate
  fails if that action builds nothing. The one exemption is `release-nros.yml`,
  the release producer, which builds cold on the host its glibc floor is
  measured on. Measured at landing: no workflow other than that one spelled a
  build.
- **Deliberately NOT counted — do not "fix" these.** A job whose `just` verb
  reaches `setup-cli` through its recipe is on the user's own road, and
  `setup-cli`'s freshness assert checks the binary against the tree it runs
  in. Manager's decision of 2026-11-13 (not a user ruling): no move onto
  the action. At landing:
  - `host-tests.yml` `feature-sweep` — `just setup linux`
  - `queue.yml` `l3` — `just setup tier2`
  - `run-matrix.yml` `matrix` — `just setup tier2`
  - `run-matrix.yml` `tier1` — `just setup tier1`
  - `run-matrix.yml` `matrix-nightly` — `just setup tier2-nightly`
  - `gate.yml` `scaffold-journey` — `just scaffold-journey`

  Jobs that run the action first and then a `just setup` verb (the rest) get
  the binary from the action, and the verb's `setup-cli` finds it fresh.

### W3 — The queue's L3 on its dedicated label

- **Owner:** nros 118.
- **Status:** DONE (2026-11-14). PR-A landed first (#3714, 2026-11-13): the
  `nros-queue` label in the doctor and provisioner, a per-role `_work`
  volume, `just check queue-runner-label`, and `runner-watchdog.yml`. PR-B
  then moved `queue.yml`'s L3 to `[self-hosted, linux, nros-queue]` once a
  runner carried that label (`nano-ros-runner-newslab-server-241-queue`:
  nros-queue, nros-sdk-zephyr, nros-big). With one queue runner, a dead one
  costs at most the watchdog's 30 minutes before the queue run is cancelled.

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
- **Status:** DONE (#3713, #3726, 2026-11-13) — W4a (`weekly.yml` on
  Sunday, the duplicate probes) in #3713, W4b (`nightly.yml` on one daily
  cron) in #3726.
  The `board-support.toml` linux note naming `probe.yml` did not move in W4b;
  it goes with W7 (below).

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
- **Status:** OPEN — W5a DONE (#3727, 2026-11-13). W5b's tool landed in
  #3731: `merge-runtime.yml` (`lane: tier1`). Issue 2475's provisioning
  defects 1-3 are fixed (#3733), defect 4 (tree-wide codegen) is
  [phase-535](phase-535-example-reorg-and-cadence-ci.md) W10, and both
  re-measures ran (43420012895 on main, 43438754804 with W10). The user ruled
  on the linux shard (below). Remains: issues 2510 and 2511, then a re-measure
  of the threadx_linux and zephyr shards to completion, final disk included,
  then their move.

  Fourth measurement, run 43551581652 (2026-11-14, main 28a9b3ebba, the
  disk balloon on): threadx_linux BUILT (105 G free after reclaim, 43 G left,
  so it fits; `check fast` reused; first fixture ~10 min in) and its run was
  refused by preconditions that do not scope a shard as its build does. The
  zephyr shard was stopped by the balloon at 2 G free: ~90 G available, 44 G
  of Zephyr builds plus ~35 G of host compile-check work a zephyr shard does
  not own. Both are issue 2548
  ([2548](../issues/2548-merge-shard-run-preconditions-unscoped.md)); the
  zephyr disk answer is re-measured after it.

  Fifth measurement, run 43568845036 (2026-11-14, main 36f665bedc, with the
  2548 build/run scope): **both shards' builds FIT.** The zephyr shard had
  ~95 G free after reclaim. Its build passed with 6.2 G left at the end, and
  its lowest point was **4.2 G free** with the 3 G balloon held. Where the
  disk went:
  - `zephyr-workspace-builds` 44 G
  - `corrosion-cargo` 23 G (the Linux rows that serve zephyr coordinates, which
    its run needs)
  - `sizes-probe` 6.3 G
  - `metadata-probe` 3.0 G
  - compile-check fixtures ~0, since host compile-checks are out of its scope

  The threadx_linux shard used ~62 G of 105 G on the previous run. The
  coordinator kept the zephyr shard hosted (no user question: it fits) and
  asked for two things against a ~4 G margin that one more example can eat:
  - the zephyr-ci image slimmed to the toolchains the index uses;
  - the disk balloon WARNS (an annotation, not a failure) when a shard's
    lowest free space is under 10 G, so a shrinking margin shows before
    ENOSPC (`scripts/ci/disk-balloon.sh --warn-free-gb`, default 10).

  The same run found two more unscoped steps (the leaf `include` arm and
  `rust-rtos-link-check`); they are issue 2548's second round.

  **User ruling, 2026-11-14 (option B): linux tier 1 stays SELF-HOSTED**
  (`run-matrix.yml`'s `tier1`). Only threadx_linux and zephyr move to hosted
  per-board shards. Measured basis: a hosted runner has ~104 G free after
  `reclaim-disk.sh`, and the linux tier-1 shard ran out of disk 97 min into its
  fixture build with 75 of its 219 rows built (run 43438754804; the first run,
  before W10, ran out after 70 min). The rejected option was splitting the linux
  shard into 3-4 hosted shards by coordinate, sized from a measurement not yet
  taken. The linux shard is therefore out of the hosted plan:
  `merge-runtime.yml` keeps it only as a measurement of the `merge` lane
  (issue 2475's "done when"), never as a tier-1 home.

  Third measurement, run 43438754804 (2026-11-13, W10 branch at 7bff8a9356,
  fixture build logs uploaded this time). Time to first fixture fell to
  38.7 / 24.3 / 16.7 min (linux / threadx_linux / zephyr), from 43.6 / 48.2 /
  49.0. The per-step split is in phase-535 W10. Every shard reached its fixture
  build and failed in it:

  | shard | job | free after reclaim | failed on |
  | --- | --- | --- | --- |
  | linux | 141.5 min | 105 G | ENOSPC writing an `nros-cpp` rmeta (`build/sizes-probe`), 97 min into the fixture build (14:45 first fixture, 16:22 failure), on row `workspace-cpp-native-talker-cyclonedds`; 77 rows had finished |
  | threadx_linux | 91.8 min | 105 G | `build-compile-check-fixtures`: two FreeRTOS cross-build rows, `freertos-kernel` not provisioned ([issue 2510](../issues/2510-compile-check-cross-rows-unprovisioned-kernel.md)); the same exit as the before-run |
  | zephyr | 64.4 min | 80 G | `west_board_import`: the image bakes SDK 0.17.4, the 3.7 workspace pins 0.16.9 ([issue 2511](../issues/2511-zephyr-ci-image-lacks-pinned-sdk.md)); 6 of 7 west fixtures built |

  **Disk, linux: does not fit, measured twice.** Both runs had ~105 G free
  when `reclaim-disk.sh` finished (109,917,100 KB on the second run), and the
  second ran out at the same file the first did, with 75 of the shard's 219
  rows built. (The `disk-report.sh` "before" figure, 74 G, is read before the
  reclaim step runs, so it is not the build's budget.) The before-run's cyclonedds
  `MultiArrayDimension_` error did not recur, so it was ENOSPC fallout, and the
  zephyr xrce row that failed before built this time. Neither is filed.
  threadx_linux and zephyr stopped on the two issues above, so their disk
  answer is still open. Their disk report was never reached: the step's
  `disk-report.sh "after just build tier1"` line sits behind the failing build.

  Second measurement, run 43420012895 (2026-11-13, main, before W10): all
  three shards REACHED their fixture build, so 2475's defects 1-3 hold. Time
  from the start of `just build tier1` to the first fixture command:

  | shard | to first fixture | tree-wide codegen | `check fast` | lane codegen (warm) |
  | --- | --- | --- | --- | --- |
  | linux | 43.6 min | 30.4 min | 10.5 min | 2.7 min |
  | threadx_linux | 48.2 min | 34.1 min | 11.1 min | 3.0 min |
  | zephyr | 49.0 min | 35.6 min | 10.6 min | 2.6 min (+9 min Zephyr prep) |

  Disk: the linux shard ran OUT. `reclaim-disk.sh` left ~104 GB free
  (109,150,560 KB) at the start of the build, and the linux fixture build
  hit `No space left on device` about 70 min in (12:42 first fixture, 13:52
  ENOSPC). So on that commit the linux tier-1 shard does not fit a hosted
  runner. threadx_linux and zephyr failed in their fixture builds for
  reasons not yet attributed (their logs were not uploaded; fixed with W10),
  so their disk answer is open. Re-check on the W10 after-run, where
  codegen is scoped to the lane. Its first
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
- **Status:** OPEN — the census and the three book-probe tracks landed
  (#3724, 2026-11-13): `lane-census.yml` and `probe.yml` are deleted and
  their four jobs run in `host-tests.yml` unchanged. Remains, all done in W7
  so `check-build` never runs in two files: `check-build` itself, the
  `board-support.toml` linux note, and the two `ci/docker/` comments that
  still name `probe.yml` (an edit there rebuilds the CI images).

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
- **Status:** DONE (2026-11-14). 14 files, not 13: `runner-watchdog.yml`
  stays its own file (below). `publish.yml` holds `docs.yml`'s `deploy` and
  `images.yml`'s `changes`, `ci-base` and `zephyr`, with their names and
  triggers. Its push `paths:` is the union of both lists (H5: a path filter
  is per file), and the `changes` job decides which job a push reaches: a
  book filter for `deploy`, one filter per image. Dispatch takes one `job`
  input (docs, ci-base, zephyr, both-images). The readers that parse the
  image jobs (`check-ci-image-inputs`, `check-ci-image-apt-packages`,
  `check-workflow-just-provisioning`) read `publish.yml`, and every live
  mention of the two old names moved. No `workflow_run` consumer named
  either. `automation.yml` DONE (2026-11-14): arm-auto-merge,
  queue-notify, pr-verdicts, roadmap-audit and nightly-report are its jobs
  `arm`, `enqueue`, `notify`, `sweep`, `audit` and `triage`, each keeping its
  job name and trigger. (`enqueue` was removed on 2026-11-14, because a merge
  group a `GITHUB_TOKEN` enqueues starts no workflow, and came back with
  `arm` on the queue GitHub App's token.) Each job's `if:` names its event (and, for
  `workflow_run`, the workflow it reads), the workflow grants no permissions
  and each job states its own, concurrency is per job, and dispatch takes one
  `job` input. Nothing consumes these workflows by `workflow_run`, so no
  consumer needed renaming. `runner-watchdog.yml` stays its own file (S4,
  manager's decision of 2026-11-14): it is the dead-runner alarm, so an error
  in an unrelated job must not silence it, and its 15-minute cron would start
  `automation.yml` 96 times a day.

### W9 — An advisory per-merge Zephyr link in the queue's L3

- **Owner:** nros 118.
- User ruling (2026-11-14, option B): add a few representative Zephyr image
  links to `queue.yml`'s L3, which is advisory and runs on the `nros-queue`
  runners that already carry `nros-sdk-zephyr`.
- **The set, chosen by coverage:** `c/talker` on `native_sim`, `cpp/talker`
  on `mps2/an385` and `rust/talker` on `native_sim`, all over zenoh on the
  3.7 line. Together they cover the three languages (the CMake C and C++
  roads and the zephyr-lang-rust cargo road), the host simulator, and one
  cross target (Cortex-M3, the mps2 board the QEMU cells use). One link per
  language and one cross target is the smallest set where a break in any
  road or in the cross toolchain shows. The 4.4 line and the rest of the
  example matrix stay in `nightly.yml`.
- **Measured** (2026-11-14, host 118, 48 cores, 3.7, all six builds green):

  | image | cold | warm |
  | --- | --- | --- |
  | `c/talker`, native_sim | 77 s | 14 s |
  | `cpp/talker`, mps2/an385 | 83 s | 22 s |
  | `rust/talker`, native_sim | 39 s | 11 s |
  | all three | ~3.3 min | ~47 s |

  L3 sweeps the example build dirs every run, so the cold column is the
  expected cost: about 20% on L3's ~17 minutes (run 43553110506: 2 min of
  setup, which already provisions Zephyr, plus 14 min of `just ci matrix
  build`).
- **Acceptance:**
  - each image is its own L3 step, named `Zephyr link: <example> on <board>
    (zenoh, 3.7)`, so a red names the image;
  - each runs on `!cancelled()`, independent of `just ci matrix build`;
  - `queue.yml` stays out of the required set, so a Zephyr red or a runner
    outage never blocks a merge;
  - the first merge-group run after landing links all three, and its L3
    time is recorded here.
- **Status:** OPEN (in review).

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

Per-merge Zephyr link coverage was put to the user, and ruled on 2026-11-14:
an advisory per-merge link in L3 (W9). L3 is not on the queue's critical path
(`queue.yml` is not a required check), and its runner already provisions the
3.7 workspace and SDK, so it costs neither of the things #3710 removed.

## Issues homed here

- [issue 2502](../issues/2502-zephyr-template-copy-out-doubled.md) — the
  Zephyr template copy-out job nearly doubled in a week (filed with #3711).
  W7 moves the job, and the slowdown goes with it.
- [issue 2499](../issues/archived/2499-verus-lane-toolchain-vstd-mismatch.md) — the
  Verus lane, resolved by #3723: its job is `verify-verus` in `weekly.yml`.
