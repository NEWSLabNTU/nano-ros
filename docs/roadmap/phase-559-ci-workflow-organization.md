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
  it moved with W7 (below).

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

  Sixth measurement, run 43582883501 (2026-11-14, main 1a54770f56, with
  2548's second round): **threadx_linux RAN its tests** (3614 run, 3037
  passed, 558 deselected out of lane). Its disk low point was ~57 G free.
  19 real failures were tests that probed the host before the lane: the
  XRCE Agent, one cmake-fixture resolver, and a feature lane with no
  coordinate in the shard. One lane-contract scan timed out at 60 s under
  load. All are issue 2548's third round. The zephyr build fit again (low
  point 4.0 G, 6.4 G left), and then its RUN step, which had no balloon, lost
  the whole job's log about 4 minutes in. The run step is now under the
  balloon; the next dispatch says where that disk goes.

  Seventh measurement, runs 43613088518 (tier1) and 43613091093 (merge),
  2026-11-14, both on #3835's branch, the first dispatch of the MERGE lane:
  - **merge × zephyr does NOT fit.** It started with 95.9 G free, and the disk
    balloon stopped `just build merge` at 1.1 G free (17:11Z), still in the
    Zephyr fixture stage. `du` at the stop:
    - `zephyr-workspace-builds` 49 G
    - `sizes-probe` 9.8 G
    - `corrosion-cargo` 9.0 G
    - the cmake workspace builds ~13 G (`cpp` 5.6 G, `violation-cpp`,
      `levels-cpp` and `derived-sched-tasks-cpp` 1.9 G each)
    - `~/.nros` 2.8 G, `target` 2.2 G, `metadata-probe` 1.9 G

    Its coordinates are zephyr c, cpp and rust over zenoh, against tier 1's
    c/xrce and rust/zenoh, and the C++ one brings the cpp workspaces. The
    shortfall is at least the rest of the Zephyr stage plus every later stage,
    so this run cannot size it.
  - **tier1 × zephyr crossed the floor too**, on the r8 image (the branch
    predates r9). It started with 95.9 G free, and the balloon stopped it at
    1.6 G free (18:00Z) in the leaves stage (`build-test-fixtures-leaves`).
    `du` at the stop: `zephyr-workspace-builds` 44 G, `corrosion-cargo` 24 G,
    `sizes-probe` 5.8 G, `metadata-probe` 3.0 G, `west-fixtures` 1.4 G. That
    is the fifth measurement's shape (44 / 23 / 6.3 / 3.0) with less than its
    4.0 G low point left. r9's 6.9 G is the first lever to measure: the next
    tier-1 dispatch on main, on r9, says whether the shard fits again.
  - **merge × threadx_linux FITS and RAN.** It started with 95.9 G free, and
    its lowest point was 6 G (the balloon's under-10 G warning fired). 3633
    tests ran: 3069 passed, 2 failed for real, and the rest were deselected
    out of lane. The feature lanes passed under the sub-lane rule, and the
    two lane-contract scans
    passed at 102 s and 107 s under their new ceiling. 2 real failures:
    - `a_rows_entry_registers_its_nodes_at_runtime` was killed at nextest's
      default 60 s while it waited (up to the lease's 240 s) on baked port
      9000, held by a ThreadxLinux `rtos_e2e` cell. The fix is a ceiling above
      the documented wait (`.config/nextest.toml`).
    - `test_threadx_linux_c_image_ends_on_one_sigterm::case_2_zenoh`: the
      image's `rclc_support_init` returned -4 against a live router. It passed
      on run 43582883501. Unexplained, so not filed from one sighting; it is
      watched on the next dispatch.
  - tier1 × threadx_linux: `just build` failed in its last gate,
    `check-action-client-arena-budget`, which examined nothing on a C-only
    lane (issue 2548, fourth round).

  **User ruling, 2026-11-14 (option a): the merge lane's zephyr shard runs on
  the SELF-HOSTED runner.** In `merge-runtime.yml`, a separate job
  (`merge-runtime-self-hosted`, same job name, run-matrix's tier-1 steps) runs
  only that shard, and the hosted matrix drops `zephyr` for `lane=merge`
  only. Everything else stays hosted where it fits: tier-1 zephyr (low point
  4.0 G), tier-1 threadx_linux, and merge threadx_linux (being measured).

  The image slim (#3837, zephyr-ci r9, published by run 43619885183):
  `prune-zephyr-sdk.sh` took the 0.16.9 SDK from **9.0 G to 2.1 G**, keeping
  arm-zephyr-eabi, x86_64-zephyr-elf and aarch64-zephyr-elf. The 1.0.1
  minimal bundle stayed 2.5 G, since it held only those already. That is
  6.9 G back from every zephyr-ci job's disk. It is measured inside the
  image, before layer compression; its effect on a shard's low point shows
  on the next tier-1 dispatch.

  Eighth measurement, runs 43650684858 (tier1) and 43650687503 (merge),
  2026-11-15, on #3889's branch (the fixed matrix), plus 43643676703 (merge,
  the #3865 tree). All three shards that should have run did run, so the
  matrix fix holds. No shard has a p95 sample yet
  (`merge-runtime-p95.py --event workflow_dispatch`: threadx_linux 0 ok,
  zephyr 0 ok).
  - **tier1 × threadx_linux FITS and PASSED.** Its lowest point was 19.9 G
    free. The sigterm test's zenoh case passed in 12.7 s. Its cyclonedds case
    was out of lane.
  - **tier1 × zephyr does NOT fit on r9 either.** The disk balloon stopped
    `just build tier1` at 1.67 G free, in `build-test-fixtures-leaves`. `du`
    at the stop: `zephyr-workspace-builds` 44 G, `corrosion-cargo` 30 G,
    `sizes-probe` 5.8 G, `metadata-probe` 3.0 G. r9's 6.9 G did not buy the
    margin back: `corrosion-cargo` grew 23 → 30 G over the fifth measurement.
  - **merge × threadx_linux crossed the floor.** The balloon stopped
    `just ci merge` at 1.78 G free, in `test-all`, so there is NO VERDICT.
    The seventh measurement had a 6 G low point on this shard. `du` at the
    stop: `corrosion-cargo` 30 G, `target` 18 G,
    `examples/workspaces/cpp/build` 8.7 G, `sizes-probe` 7.6 G.
  - **merge × zephyr (self-hosted) RAN**, twice. Each run ran 3638 tests with
    1 real failure, the same one both times: `sched_dims_applied`'s
    `CorePinPlacement/zephyr/c`, which fails with `qemu-system-aarch64: failed
    to find romfile "efi-e1000.rom"`. The runner image lacked `ipxe-qemu`
    (an image fix, in flight).
  - `cargo-dir-dupes` on `build/corrosion-cargo`, at the end of the two
    hosted shards that ran out of disk: 80 cargo dirs. 601 / 456 units are
    stored in more than one dir, and the extra copies come to 35.5 / 36.6 G
    by apparent size. Nearly all of it is the metadata probe's
    `nros-cpp-image_*` dirs, and mostly their HOST half. This is W10's
    subject.

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
- **Status:** DONE (#3724 2026-11-13, rest with W7 2026-11-15) — the
  census and the three book-probe tracks landed (#3724, 2026-11-13):
  `lane-census.yml` and
  `probe.yml` are deleted and their four jobs run in `host-tests.yml`
  unchanged. The rest landed with W7 so `check-build` never ran in two files:
  `check-build` itself is `host-tests.yml`'s (three shards, same steps), and
  the `board-support.toml` linux note names `host-tests.yml`'s book probes.
  `ci/docker/zephyr-ros/Dockerfile`'s comment was handled in its own PR.
  LEFT: `ci/docker/ci-base/Dockerfile`'s comment still names `probe.yml` —
  an edit there rebuilds the CI base image, so it waits for the next change
  that rebuilds it anyway.

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
- **Status:** DONE (#3946, 2026-11-15). `gate.yml` has no
  `schedule` (push, pull_request, merge_group, workflow_dispatch remain), and
  every `if:`/`sibling` event list in it dropped `schedule` with it.
  `check-build` (three shards, same steps: compile-check fixtures, bindings
  cache, disk reports and transcripts, `just check build`, `no-std` and
  `pristine-checkout` on shard 3) is `host-tests.yml`'s, on its 03:00 cron
  and dispatch, with gate.yml's no-DWARF `CARGO_PROFILE_*_DEBUG` variables as
  job env. `check-template-copy-out-zephyr` is `nightly.yml`'s
  `template-copy-out-zephyr` job, on its 05:00 cron and dispatch; it saves
  `main`'s SDK-store entry itself, so `warm-sdk-store` dropped its
  `copy-out-zephyr` kind. `CI` no longer needs or inspects either job.
  `check-merge-group-gates`' `DEFERRED_PARTS` names `nightly.yml` /
  `template-copy-out-zephyr`; rule 6 accepts the deferred part only from that
  job on a daily (`M H * * *`) cron and refuses the part anywhere in
  `gate.yml` (self-test: the old placement, a weekly cron, a renamed job, a
  step without the part, among 28 cases). `check-default-gates-run-somewhere`
  gained R4, refusing a `schedule` trigger on the workflow holding `CI`
  (self-test, and measured red against the pre-W7 `gate.yml`). The
  merge-group run is green: `gate` run 43699058961
  (`gh-readonly-queue/main/pr-3946-781c5b10de`), every `check /` job and `CI`
  success, and `CI` lists no job that left `gate.yml`. The moved jobs ran on
  their own crons the next day: `host-tests.yml` run 43796857800 (2026-11-16,
  `build tier + no-std` 1/3-3/3 success) and `nightly.yml` run 43808375964
  (2026-11-16, `template-copy-out [zephyr]` success; that run's two reds,
  `stack-floor drift` and `FreertosMps2`, are other jobs).

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
- **Status:** DONE (2026-11-14, #3818). The first L3 run with the links,
  43586752627 (merge group for #3823, on `server-241-queue`), linked all
  three, green: `c/talker` 79 s, `cpp/talker` 68 s, `rust/talker` 43 s,
  3 min 10 s together, in line with the measurement. L3 took 24 min 50 s in
  all: setup 4 min 10 s (the first run after the runner came back) and
  `just ci matrix build` 16 min 17 s. The Rust link needed no image change;
  clang is in the runner image. Until 10:49Z the links never ran: the
  `nros-queue` runner was gone from 05:08Z (fixed by
  runner-store's container-owned dir rule and caught next time by the
  presence watchdog).

### W10 — Lever 2: the metadata probe shares one host half per key

- **Owner:** nros 175. The user approved building it on 2026-11-15, safety
  tests first, and after the measurements ruled that it does not turn on (below).
- **Why.** Both hosted shards that ran out of disk in W5b's eighth
  measurement held `build/corrosion-cargo` at 30 G (`du`), and the disk
  balloon stopped them 0.2–0.3 G under its 2 G floor. Across that tree,
  `scripts/ci/cargo-dir-dupes.py` finds 456–601 units stored in more than one
  cargo dir: 35.5–36.6 G of extra copies against a 40.1–40.4 G total, both by
  apparent size, so about 90 % of it. Almost all of it is the metadata probe's
  per-workspace `metadata-probe/<key>/nros-cpp-image_<hash>/` dirs
  (0.6 G each, 0.5 G of it host half). The largest repeats are `zpico-sys`'s
  build-script dirs (0.8 G over 38–39 extra copies, two hashes) and
  `libnros_macros-*.so` (0.7–0.8 G over 36–38). So the margin both shards
  lacked is a small fraction of what this one directory duplicates. Whether
  sharing makes the shards FIT is a separate question, answered by the
  acceptance below and not assumed here.
- **The units are already one unit.** A cargo unit's file name carries its
  metadata hash (features, deps, profile and the path spelling it was reached
  by). The same `libnros_macros-<hash>.so` sits in 36+ dirs, so those dirs
  agree on every input, the spelling included. Lever 1 of
  [issue 2318](../issues/2318-cmake-leaf-recompiles-host-tooling.md) (the
  probe builds without debuginfo, 2026-11-11) made each copy smaller. It did
  not change how many copies there are. That issue measured the count as
  bounded by workspaces, not by keys.
- **Proposal.** Each probe key gets one shared cargo `build.build-dir`
  (intermediates: `deps/`, `build/`, `.fingerprint/`). Each workspace keeps
  its own `target-dir` (uplifted artifacts). Cargo uplifts every root
  `nros-{c,cpp}-image` staticlib by crate name, so a shared `target-dir` would
  let concurrent probes overwrite each other's archive. A per-workspace
  `target-dir` keeps that out of reach. `build.build-dir` works on the pinned
  toolchain without `-Z` (2318, checked on a scratch crate). The fixture images
  are out of scope; they keep one directory per leaf.
- **What it must not reopen: [issue 0616](../issues/archived/0616-ws-mixed-duplicate-nros-platform-global-allocator.md).**
  A cargo dir serves one workspace root because two roots sharing one can
  hold two units of one crate that differ only in path spelling, and a
  transitive `-L dependency=` lookup then binds either. `nros-platform`'s
  `#[global_allocator]` made that a duplicate-symbol link failure, which was
  intermittent while the cause was permanent. A shared build-dir puts every
  workspace of a key in one `deps/`, which is exactly that state, **unless**
  every non-root crate is reached by the same spelling in every workspace.
  That is why 2318 parked lever 2 on "a duplicate-symbol link test under
  parallel leaves".
- **What proves it safe.** Each item is a measurement or a gate. None is an
  argument.
  1. *One unit per crate per key, as a precondition and not a hope.* Only
     a root that `nros_image_crate` generated joins the shared build-dir
     (`nros_image_share_build_dir`). Its manifest has the image crate as its
     only member, and every other package is an external path dep, spelled
     absolutely and therefore the same in every workspace. The API crate
     imported from the repo root does not join, and neither does anything
     else: it keeps its own dir and the configure says why.
     **The roots need an identity too, and the proposal missed it.** Measured
     on cargo 1.99: two roots with one name and version share ONE unit hash,
     because cargo spells a member relative to its own root. The second
     build found the unit fresh and uplifted the FIRST root's archive, with
     no error. Every generated root is named the same, so each one now
     carries a version suffix `0.0.0+r<id>` and an embedded marker string,
     both from a hash of its own build dir. The suffix separates the root
     units while dependencies stay shared.
     After every probe build, `nros dev shared-build-dir-check` refuses two
     things. The first is a fingerprint group, identical in every field
     except the path, that holds two paths (0616's shape). The second is an
     uplifted archive that does not carry exactly its own marker. Neither the
     uplifted `.d` nor the fingerprint dep-info can see the collision: both
     spell the source as the relative `src/lib.rs`.
  2. *The 0616 link test, with its negative control.* Probe every C and C++
     workspace of one key IN PARALLEL into the shared build-dir, `mixed`
     included (the entry that caught 0616 both times), N times. Assert that
     each linked probe binary defines `__rust_alloc` and every `#[no_mangle]`
     export once (`nm`), and that each workspace's sidecars are byte-identical
     to a solo build into its own dir. The negative control is 0616's shape
     rebuilt: two roots that reach `nros-platform` by different spellings,
     sharing one dir. The test must go red on it, or it proves nothing.
  3. *No uplift collision.* Each workspace's uplifted `nros-{c,cpp}-image`
     archive hashes the same after the parallel run as after a solo build.
  4. *Cost of the lock.* Measure whether cargo's build-dir lock serialises
     the parallel probes, as wall time against today's per-dir run. A
     serialised probe may cost more time than it saves disk, and the numbers
     go to the user either way.
  5. *A shared build-dir is a LOCK, so nothing inside a build may inherit it.*
     Cargo holds an exclusive lock on its build-dir for `build`, `check` and
     `rustc`, and holds it while it waits on a build script. nros-c's size
     probe runs a nested `cargo build` from its build script. With
     `CARGO_BUILD_BUILD_DIR` inherited, that nested build blocked on the lock
     its parent held: the first in-place smoke run hung 3 h, with no error.
     Measured on cargo 1.99: `metadata`, `locate-project` and a running
     `cargo test` do not hold the lock, so the class is code that runs
     inside a build. Every cargo that code runs now comes from
     `nros_sizes_build::nested_cargo()`, which removes the variable.
     `check-nested-cargo-build-dir` refuses any other spelling in every
     `build.rs` and build-dependency crate, a scope it derives from the
     manifests. `just check shared-build-dir` runs a real nested build under
     a shared build-dir with a timeout, and keeps the deadlock as its
     negative control.
  6. *Count real bytes.* `cargo-dir-dupes.py` counts apparent `st_size`
     without merging hard links. Its 40 G total against `du`'s 30 G on the
     same tree shows it overstates. Before and after are read in allocated
     blocks with one count per inode, so the saving is real disk.
- **Acceptance.** Items 1–3 and 5 green, plus the existing link gates green. On
  the tier1 × zephyr hosted shard, `build/corrosion-cargo` falls by the
  probe's duplicate bytes (`du` before and after, same commit). That shard and
  merge × threadx_linux reach a verdict above the disk balloon's floor, or
  the remaining shortfall is measured and named.
- **Measured 2026-11-15** (20-core host, about 40 load from other users).
  `examples/workspaces/{c,cpp,mixed}` were probed CONCURRENTLY, each run from a
  fresh `NROS_BUILD_ROOT`. Baseline: today's tree (the per-workspace build
  dir), alternating with W10, 4 rounds each, plus a solo W10 run per workspace.
  - Items 1–3: every run probed all 23 components (`no producer` = 0).
    Each of the 22 probe binaries per tree defines `__rust_alloc` and
    `nros-platform`'s `GlobalAlloc::alloc` exactly once, with no duplicate
    `T`/`D` symbol. All 46 sidecars equal the baseline's except
    `provenance.inputs_digest`, which hashes the CLI and sources, so it
    differs between two trees by construction. All 6 uplifted image archives
    hash the same after each parallel round as after a solo build. The
    in-probe check said "safe to share" every time.
  - Item 4, the lock costs wall time. Parallel probe of the three workspaces:
    baseline 119 / 86 / 72 / 80 s, W10 121 / 125 / 102 / 114 s. Warm rounds
    (2–4) average 79 s against 113 s, about +40 %. Run serially, the three
    solo W10 runs sum to 223 s, so the probes still overlap. They wait on the
    build-dir lock only where they share units.
  - Item 6, real bytes: `du` in allocated blocks, hard links counted once.
    The probe's `corrosion-cargo` falls from 2301 to 730 MiB (−68 %), and the
    whole build root from 3666 to 2095 MiB, for these three workspaces. Two
    keys serve them.
  - Item 2's negative control is the census refusing 0616's shape on real
    cargo (`just check shared-build-dir`, `spellings`). It is not a real
    duplicate-symbol link of `nros-platform`, so the refusal happens before
    any link.
- **Ruling, 2026-11-15:** after these measurements, the user ruled that
  build-dir sharing goes NOWHERE yet, not even on the hosted shards. The
  hosted Zephyr shard's disk is to be solved another way (W12). What stands on
  its own lands, with sharing OFF:
  - The nested-cargo lock fix (`nros_sizes_build::nested_cargo()`) and its
    gate `check-nested-cargo-build-dir`. Any future shared build-dir needs it,
    and so does any build that sets `CARGO_BUILD_BUILD_DIR` itself.
  - The census, with the source-identity fix, plus the marker check and
    `nros dev shared-build-dir-check`.
  - `just check shared-build-dir`'s real-cargo scenarios, including the
    nested-cargo deadlock control.
  - `nros_image_share_build_dir` and the root identity, both inert unless
    `NROS_SHARED_CARGO_BUILD_DIR` is ON. The probe passes OFF explicitly and
    no lane turns it on. Off, the generated image crate is byte-for-byte what
    it was, with no version suffix and no marker bytes in any image.
  Picking the option up later is a one-line change in
  `metadata_probe_cmake.rs` plus a new ruling. The measurements above are
  the case for and against it.
- **Status:** DROPPED (the user's ruling, 2026-11-15; safety work lands, sharing OFF)
  The sharing itself goes nowhere yet, and the hosted shard's disk moves to
  W12. Issue 2318 stays homed in phase-489; this was its lever 2, scoped to
  the probe.

### W11 — A failed metadata probe is a failure, not "no producer"

- **Owner:** nros 175. Found by W10's smoke test, and fixed separately from
  W10.
- **What.** [Issue 2608](../issues/archived/2608-metadata-probe-failure-reads-as-no-producer.md):
  `nros configure` and `nros dev bindings` report a probe whose BUILD failed as
  "no producer for <node>", print `sync: done` and exit 0. On
  `examples/workspaces/c`, every probe failed twice that way. The two facts
  get reported apart, and a failed probe exits non-zero naming the node and
  its first error line. The genuine no-producer fallback stays as it is.
- **Acceptance.** A probe build broken on purpose (an unprovisioned source)
  exits non-zero and names the node and the error. A workspace with a real
  no-producer component still syncs with exit 0. Every caller of
  `refresh_stale_sidecars` is checked before the exit code changes.
- **Ruling (manager, 2026-11-16):** issue 2520's knob joins this item.
  `NROS_PROBE_FAILURE=warn` stays the default for an interactive `nros build`
  / `configure`, but even `warn` refuses a run where no probe succeeded.
  `refuse` holds wherever nobody reads the warning: the three fixture
  builders, every `just` lane, and the release dry run. The warning's last
  line carries the failed count.
- **Status:** DONE (the PR that lands this line, 2026-11-16).
  `RefreshReport` keeps `failed` (probe build failures, recorded failures, a
  whole-project configure failure, a probe-root mismatch), `degraded` (a
  deploy-bound example's host probe, #0288's designed fallback) and
  `unsupported` (no producer) apart. `refuse` is exported by the root
  `justfile` (module recipes inherit it), forced in
  `scripts/build/{fixtures,workspace-fixtures,compile-check-fixtures}-build.sh`,
  and set in `release-nros.yml`. Callers: `refresh_stale_sidecars` has one,
  `cmd/ws.rs`, reached from both sync roads; the single-package road still
  writes the leaf settings before it fails. Measured:
  - Every tracked workspace (`examples/workspaces/*`, 18) ran `nros configure`
    under `NROS_PROBE_FAILURE=refuse` on the W11 branch: all exited 0, 87 probes
    rebuilt, 0 failed, so no fixture lane goes red from this change.
  - Broken on purpose (`#error` at the top of `workspaces/c`'s `Talker.c`):
    under `warn` the run exits 0, prints `probe FAILED for talker_pkg::talker:
    …Talker.c:1:2: error: #error broken on purpose…` with the marker as the full
    error, and its last line before `sync: done.` is `1 metadata probe(s)
    FAILED`. Under `refuse` it exits 1 with the same node and cause, through
    issue 1469's recorded-failure path (the source unchanged since the first
    run).
  - The negative control: a no-producer component and a deploy-bound degrade
    are not failures under either policy (unit tests). A mutation that drops
    the `refuse` branch fails a test.

### W12 — The hosted Zephyr shard's disk, without sharing a build-dir

- **Owner:** nros 175. It follows W10's ruling (2026-11-15) that build-dir
  sharing goes nowhere yet.
- **Ruling, 2026-11-15: option A.** Between the build and run stages, evict
  `build/corrosion-cargo/metadata-probe/` and `build/metadata-probe/`, and keep
  `sizes-probe`. Guard it: if the run stage ever reads one of those dirs, the
  eviction must fail loudly. Acceptance: a lane=tier1 dispatch on the branch
  reaches a VERDICT and records the freed bytes (a real `du`, replacing the
  derived 20 G) and the lowest free space. If that margin is under 10 G, add B
  (no debuginfo for that shard's test binaries) in the same PR and say so.
- **What the shard runs out of, measured.** Run 43684256518,
  `merge-runtime tier1 (zephyr)`, 146 G disk:
  - `reclaim-disk.sh` freed 32.3 G of preinstalled toolchains before the
    build: android 12 G, dotnet 6.3 G, `/__t` 5.4 G, ghcup 3.7 G, swift
    3.5 G, powershell 1.4 G.
  - `just build tier1` then FIT. It left 9.4 G free, 7 G at its lowest.
  - `just ci tier1 run` compiled the test binaries next, under the
    debuginfo profile `nros-relwithdebinfo`. `target/` grew 2.2 → 4.2 G
    and `target-zpico-multisession` reached 2.4 G. The balloon holds 3 G in
    reserve, and free space crossed its 2 G floor during `test-all`. The
    first three tests had passed; the shard has no verdict.
  - The run wrote about 7.5 G, so the shard is short by roughly **5–8 G**,
    not by the 30 G W10 aimed at.
  - `build/` at that moment (the run's `nros-disk-exhausted.txt` artifact):

    | Directory | Size (du) |
    |---|---|
    | `zephyr-workspace-builds` | 44 G |
    | `corrosion-cargo` | 24 G |
    | `sizes-probe` | 6.4 G |
    | `metadata-probe` | 3.0 G |
    | `west-fixtures` | 1.4 G |

    `cargo-dir-dupes.py` reads `corrosion-cargo` as 32.4 G apparent. 28.2 G
    of that is extra copies in the probe's `nros-probe` profile, and the
    25 largest cargo dirs are all `metadata-probe/…`.
- **Options.** Each has its bytes and what it costs.

  | # | Option | Frees | Cost or risk |
  |---|---|---|---|
  | A | Evict the metadata probe's outputs between `just build` and `just ci tier1 run`: `build/corrosion-cargo/metadata-probe/` and `build/metadata-probe/` | about 20 G + 3.0 G (the first is derived: 28.2 of 32.4 G apparent is probe copies, scaled by du/apparent = 24/32.4; measured exactly on the dispatch) | The run must not read them. It doesn't by the code: the staleness walk (`find_build_script_outputs`) reads `corrosion-cargo/<platform>/` for CROSS leaves only, and the sidecars live in `src/*/metadata/`. Confirmed only by the acceptance dispatch. `sizes-probe` (6.4 G) is NOT evicted, because the run's own cargo builds may re-enter it |
  | B | Build the run stage's test binaries without debuginfo on this shard | at most the 4.4 G those dirs hold (`target/` +2.0 G, `target-zpico-multisession` 2.4 G); the real saving is unmeasured | Worse backtraces on that shard only. On its own it may not cover 5–8 G |
  | C | Split the shard into a build job and a run job | the run job starts on an empty disk | It needs the built images moved between jobs (their size is unmeasured; `zephyr-workspace-builds` is 44 G with intermediates), plus a second job's queue and setup time. The most work |
  | D | Reclaim more preinstalled software | unmeasured: 32.3 G is already gone, and what is left needs a `du` on a dispatch | Each item is a bet on the runner image |
  | E | Move tier1 zephyr to the self-hosted runner | no hosted ceiling | It uses the self-hosted runner's capacity, and the tier1 lane runs on hosted runners by design (W5b) |

- **Recommendation: A**, measured before it lands. One step after `just
  build` removes the two directories, after `git check-ignore` proves each
  path is ignored and with `du` before and after. Acceptance is a lane=tier1
  dispatch of `merge-runtime.yml` on the branch:
  - the zephyr shard reaches a VERDICT
  - the balloon's lowest free space is reported
  - the freed bytes are recorded

  If that margin is under 10 G, add B and measure again. A frees about
  3–4× the shortfall and changes nothing the run reads, by the code.
  Unlike W10 it changes no build.
- **Implementation.** `scripts/ci/evict-build-stage-only.sh` runs from two
  zephyr-only steps in `merge-runtime.yml`:
  - `evict`, between `just build` and `just ci … run`. It runs `git
    check-ignore` on each dir, then `du` (allocated blocks), then `rm -rf`. It
    leaves a read-only sentinel FILE of the same name, so anything in the run
    that reads or creates under the path gets `ENOTDIR` and fails at the
    point of use, through a workspace's `cargo` symlink too.
  - `verify`, after the run, under `if: always()`. It fails if a sentinel
    was replaced or removed.

  `selftest` is the negative control. A read through a symlink into the
  evicted path must fail, a write under it must fail, and a replaced sentinel
  must fail `verify`. It runs before every `evict`.
- **Acceptance, measured (run 43716062858, head 1d968501f6, 2026-11-15).**
  The zephyr shard reached a VERDICT.
  - Freed, by `du`: `build/corrosion-cargo/metadata-probe` **24 G** and
    `build/metadata-probe` **3.0 G**, 27 G in total. That replaces the
    derived "about 20 G + 3.0 G". Free space went 9.8 G → 37 G.
  - Lowest free space during the run stage: **10.8 G**, at the end of
    `test-all`. The balloon was never exhausted (`exhausted=0`). The build
    stage, which W12 does not change, still bottoms out at **7.7 G** and draws
    the balloon's under-10 G warning.
  - `verify` passed: the run stage touched neither evicted path.
  - **B is not added.** The run-stage margin is 10.8 G, at the ruling's 10 G
    line, not under it. B would only shrink the run stage's test binaries,
    so it cannot raise the build stage's 7.7 G either. The build-stage margin
    is a separate question, and W12 does not answer it.
  - The verdict was red on ONE real failure, unrelated to the eviction:
    `weak_symbol_audit::owned_weak_symbols_are_audited` hit nextest's 60 s
    timeout. It is a scan of tracked sources, 31 s on the threadx shard of the
    same run and 9 s locally, filed as issue 2613. The run's other red, the
    `threadx_linux` shard's `test_rtos_pubsub_e2e` C cell (`app_main` exits
    156), is on main and is not the zephyr shard's.
- **Status:** DONE (#3997, 2026-11-15). The acceptance dispatch's verdict
  is above: 27 G freed by `du`, a 10.8 G run-stage low, `verify` passed, and
  B not added. Nothing it found is left without a home: the
  `weak_symbol_audit` timeout is issue 2613, and the build stage's 7.7 G low
  is W13 (parked until measured as a problem).

### W13 — The hosted Zephyr shard's BUILD-stage disk margin (low priority)

- **Owner:** nros 175. Added at the manager's direction (2026-11-15). **No
  work until the margin is measured as a problem.**
- **What is known.** W12's acceptance run (43716062858) gave the run stage
  37 G, with a low of 10.8 G. The build stage, `just build tier1` on the
  zephyr shard, is not changed by W12. It still bottoms out at **7.7 G**
  free and draws the balloon's "fell to 7 GB at its lowest (warning below
  10 GB, stop at 2 GB)" warning. Run 43684256518 measured the same stage at
  7 G, so that is two runs and no failure.
- **Trigger.** Pick this up when a build stage stops on the balloon's 2 G
  floor, or when its low falls under 5 G on two runs. Those runs are the
  measurement. Until then the warning in the job log is enough.
- **Options, when triggered.**
  - **C:** split build and run into separate jobs.
  - **D:** reclaim more preinstalled software, priced with a `du` on a dispatch
    first.
  - Evict build-only intermediates MID-build. That needs to know which of
    `zephyr-workspace-builds`' 44 G the later rows reread.

  W12's B does not apply: it changes only the run stage's test binaries.
- **Status:** OPEN (parked until triggered)

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
  `zephyr-copy-out` plus the daily `template-copy-out-zephyr` (gate.yml's until W7) build
  copied-out projects. A Zephyr link regression is caught within a day.
- **phase-535 W8 item 4 (2 lines × 3 languages with native_sim runtime) does
  not change the link answer.** Daily link coverage already exists. W8.4 adds
  RUNTIME (native_sim cells) and reshapes the matrix; it closes issue 2462
  ("Zephyr daily cadence has no runtime job"), not a link gap.

Per-merge Zephyr link coverage was put to the user, and ruled on 2026-11-14:
an advisory per-merge link in L3 (W9). L3 is not on the queue's critical path
(`queue.yml` is not a required check), and its runner already provisions the
3.7 workspace and SDK, so it costs neither of the things #3710 removed.

### W14 — A lane shard's run step runs the lane's cells, not `test-all`

- **Owner:** nros 175. Added at the manager's direction (2026-11-17), from
  [issue 2475](../issues/2475-merge-lane-fails-before-fixtures.md)'s cost
  split.
- **What.** On a hosted lane shard, `just ci merge` / `just ci tier1 run` is
  `test-all`. On threadx_linux (run 43982551835) that was 22.8 min: five
  feature lanes compiled and ran (about 12 min, `loan_e2e` alone 7.5), then
  the whole workspace's unit suite (3,689 tests, about 4 min) and its
  doctests (about 3.5 min). The unit suite, doctests and feature lanes
  already run per merge group (`gate.yml`'s `test-unit` and `check-build`),
  and the lane's own cells are a small slice of the run. The shard runs the
  lane's cells only, scoped by its coordinates (a `just test <lane>`).
- **Acceptance.** One nightly dispatch before and one after, on the same
  board, gives the run step's time for each. Every cell the lane selected
  before still runs after (`check-skip-budget`'s counts agree), and no gate
  loses its only lane (`check-default-gates-run-somewhere`).
- **Status:** OPEN

### W15 — A lane shard's census and fixture build cover only its coordinates

- **Owner:** nros 175. Added at the manager's direction (2026-11-17), from
  issue 2475's cost split.
- **What.** The same run spent 12.6 min in the census prepass (7.1 of them on
  `workspaces/c`) and 44.6 min in the pooled fixture build, which also visits
  rows the merge lane's cells do not run: `bins/logging-smoke-threadx-linux`,
  `condvar-contract-threadx-linux` and `pool-exhaustion-threadx-linux` are
  not Example cells. The census and the build take the shard's coordinate
  file, as the build's row filter already does (`lane-coords`). Whether the
  pooled `linux` stage builds only the rows that serve the threadx cells
  (issue 2548's observer) or the whole linux slice is measured first, from
  one verbose run.
- **Acceptance.** Before and after on one nightly dispatch: census and build
  time per shard, and every cell that ran before still finds its fixture.
- **Status:** OPEN

## Issues homed here

- [issue 2502](../issues/2502-zephyr-template-copy-out-doubled.md) — the
  Zephyr template copy-out job nearly doubled in a week (filed with #3711).
  W7 moves the job, and the slowdown goes with it.
- [issue 2499](../issues/archived/2499-verus-lane-toolchain-vstd-mismatch.md) — the
  Verus lane, resolved by #3723: its job is `verify-verus` in `weekly.yml`.
- [issue 2625](../issues/2625-gates-exceed-their-cpu-weight.md) — weight-1
  check::build gates that run up to 22 processes (the token pool's over-weight
  report). Parked: fix one only when it measures as a problem, the way
  `cli-tests` was fixed (`cpu:4`, 2026-11-16).
- [issue 2676](../issues/2676-gated-worktree-test-binary-debuginfo.md) — one
  cold `just ci gate` grows a worktree to 185 GB, and its test binaries with
  full debuginfo are about 50 GB of that. Every host hit its disk floor on
  2026-11-17.
- [issue 2669](../issues/archived/2669-hook-repo-side-effects-gate-unbounded-runtime.md) — RESOLVED 2026-11-17 —
  `check-hook-repo-side-effects` took 98 s to over 1,900 s on the fast line
  on one day, and its per-script probes have no time limit. It cancelled the
  merge group for #4125. Fix: a per-probe timeout that fails naming the
  probe, plus per-probe timings.
