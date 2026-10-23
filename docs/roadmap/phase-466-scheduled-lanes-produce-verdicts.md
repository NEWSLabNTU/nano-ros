# phase-466 -- every scheduled lane produces a verdict again

**Status (2026-10-22). The blackout is over: every scheduled lane reached its
own work in the last week, and what is red now is red about something.**
Measured over the scheduled runs of 2026-10-17 to 10-21:

| lane | reaches its work? | what is red |
| --- | --- | --- |
| `probe`, `lane-census`, `pr-verdicts` | yes | nothing; green every run |
| `gate` | yes | alternates green and red |
| `run-matrix` (tier 1 + tier 2) | yes, 7 of the last 8 runs (`just matrix-triage`) | the same 3 cells in both tiers: trace-marker tests that #1949's log threshold silenced, fixed in their own PR |
| `nightly` | yes, 12 of 16 jobs pass (`just nightly-triage`) | only tier-2 pairwise, whose build died on `No space left on device`, then `ld … signal 7 [Bus error]` and rustc SIGSEGV after the fixture stamp was written — the runner's disk, issue 1353's shape |
| `live-peer` | 10-17/10-18 green; 10-19 to 10-21 red in its build | `zpico_ring_claim_head` declared `ptrdiff_t`, defined `intptr_t` (#1956's change) — fixed by `948bd88ec9` (issue 1979) AFTER the last run |
| `host-tests` | 10-18 green | the feature sweep still built `nros-node[std]` (fixed by `161dc7f444`, after the last run), plus ENOSPC on the runner |

So the acceptance below holds for every lane in the measurement table but
`live-peer` and `host-tests`, and both of those have their code cause fixed on
`main` and await one scheduled run. The issue sweep of the same day resolved 12
of the homed issues on commit + run evidence (each resolution names both):
1136, 1158, 1355, 1364, 1474, 1493, 1497, 1579, 1628, 1665, 1666, 1755.
Still open here: 1325 (the filter's haystack still holds `$conf_files`), 1483
(`[python.empy]`/`[python.lark]` still listed), 1758, and 1651/1365/1842/1459
for their remaining items; 1375, 1457 and 1899 show no symptom in the last runs
but name no fix, so one more clean run decides them. W5 is still open.
**Left before archiving:** one green scheduled `live-peer` and `host-tests`,
the trace-marker fix, the runner-disk exhaustion (an image/runner fix, not code), W5, and
the four issues above.

**Status (2026-10-13). Provisioning passes; three build faults found, two
already fixed.** The 2026-10-12 nightly (run 38213993313, `3df774a34c`) reached
the build in both matrix lanes and stopped there. Tier 2's `freertos` and
`threadx_linux` stops were phase-484 W3c readers of `FREERTOS_DIR`/`THREADX_DIR`
that no longer had them exported. 6c98b7510f and 97d38d46e7 fixed both, after
that run's checkout. Tier 1's stop was the `tier-priority-plan-image` gate
calling `build-ws-rs-realtime-derived-entry-zenoh` STALE. The image is current:
its `CONFIG_NROS_ZENOH_READ_PRIORITY=100` is stated by `derived_bringup`'s
`system.toml`, which phase-484 W5b renders as a `default` in
`<build>/nros/Kconfig.from-system-toml`, a source the staleness rule did not
read. Reproduced locally (`just build tier1`), fixed in
`scripts/lib/priority_plan.py`, with two self-test cases. The same run also
showed that the gate's verdict never reached the lane log: west-fixtures prints
after it, and the job keeps only the tail of `zephyr.log`. The recipe now
repeats the verdict where it fails.

**Status (2026-10-12). All four waves LANDED (#1209–#1212). Acceptance waits
on issue 1364**: the zephyr CI image is Python 3.10 with no TOML parser, so the
lanes that read a manifest in it still cannot report. Archive this phase once a
scheduled run of each lane produces a verdict.

**Status (2026-09-23). IN FLIGHT, four waves, all four claimed and running.**
This phase was not planned; it was opened from a measurement. Every scheduled
lane except `pr-verdicts` has failed on every run for eleven days, so the
project's whole breadth coverage -- every platform, every RMW, every interop
pair, the book's bootstrap probe -- has produced no verdict since 2026-09-11.
The goal is not "the lanes are green"; it is that each lane can once again
tell a new regression apart from yesterday's failure, which is the property
CLAUDE.md's "a red CI lane answers one of two questions" paragraph says a
uniformly red lane does not have.

## The measurement

Scheduled runs, window 2026-09-11 to 2026-09-23, read from `gh run list
--event schedule --limit 100`:

| lane | runs | failed | last success in window |
| --- | --- | --- | --- |
| `nightly` | 25 | 25 | none |
| `live-peer regression` | 13 | 13 | none |
| `run-matrix` (tier 2) | 13 | 9 | none |
| `gate` | 12 | 12 | none |
| `host-tests` | 12 | 12 | none |
| `probe` | 1 | 1 | n/a -- see below |
| `pr-verdicts` | 24 | 0 | 2026-09-22 |

The only green scheduled lane is the one that reports on pull requests. That
is why this went unnoticed for eleven days: PRs kept merging, the required
`CI` context kept passing, and every lane that would have said otherwise was
already red.

**`probe` is the one row that is not an eleven-day blackout, and the first
version of this document got it wrong.** The workflow landed 2026-09-21
(`a625ef279`, phase-452 W3) and its cron is daily at 08:00 UTC, so it has had
exactly ONE opportunity to run and spent it on the 127. Its cadence is
correct and its single-run count is the workflow's age, not a symptom. The
narrow fix had also already landed before this phase opened -- `9722fca32`,
2026-09-22 08:50 UTC, 34 minutes after the failure -- so W2 changed
`probe.yml` not at all. Five lanes are dark; the sixth is new.

Six issues describe pieces of this and none of them describes the whole:
[1158](../issues/archived/1158-tier2-lane-has-produced-no-verdict-for-six-days.md) (tier 2, filed
2026-09-06, seventeen days ago), 1359, 1457, 1458, 1459, 1034.

## What the triage found

Six distinct failing steps, and **four of the six are the runner environment
rather than the code**:

| lane | failing step | measured cause |
| --- | --- | --- |
| `nightly` zephyr jobs (22) | `Set up Zephyr 3.7/4.4 workspace` | `unzip` absent -> `setup-clang-format` fails -> `_setup-common` fails |
| `probe`, both tracks | `just probe checkout` / `installed` | `just: command not found`, exit 127 -- narrow fix already landed in `9722fca32` before this phase opened |
| `live-peer` | `Build the fixtures those rows resolve` | `ModuleNotFoundError: No module named 'tomllib'` / `'tomli'` |
| `run-matrix` tier 2 | `just build tier2` | `west-fixtures.sh` never passed `-DZEPHYR_EXTRA_MODULES`, so 4 of 5 west fixtures died at Kconfig on undefined `CONFIG_NROS` |
| `nightly` tier 2 | `just build tier2-nightly` | `ModuleNotFoundError: No module named 'catkin_pkg'` -- the self-hosted runner's rosidl deps, a HOST PROVISIONING gap |
| `gate`, `host-tests` | `just check build` (reached via `just ci tier1` for host-tests) | THE SAME STEP, on a full disk -- issue 1353 |

**Corrected 2026-09-23, after all four waves measured their own lanes.** The
table above is the second version; the first named a cause for every row and
was wrong in three of them, in the direction that reads as understanding.
What each wave actually found is below, and the pattern is worth stating
once: **every one of my three guesses pointed at the code, and every real
cause was the environment the code ran in.**

* `msg2idl.py` was never about the `.msg` file. The adapter never reads its
  argument; it fails on `import catkin_pkg` before it gets there. My reading
  and issue 1457's both blamed `builtin_interfaces/msg/Duration.msg` and its
  `/tmp/tmp*/` staging tree, which have nothing to do with it. The traceback
  was in the job log the whole time, in the module's `log tail` block -- what
  hid it is the fixture runner's report, which quotes lines matching `error:`
  and never the lines above them.
* `bkpt` was a red herring. It sits in the `nros new -> sync -> resolve` job,
  which PASSES; it is a non-fatal `sync:` degradation line. It was real
  enough to file (issue 1265 gained a third shape), but it is not why any
  lane is red.
* `zenohd` was a red herring. Unreachable -- the lane dies in `just check`,
  upstream of `test-all` entirely.
* **The CI image is not implicated in `gate` or `host-tests` at all.** Both
  pull `nano-ros-ci:humble`, every provisioning step in both succeeds, and
  their failures begin **2026-09-03**, before the 2026-09-12 image build.
* **The image timing does not explain the Zephyr lanes either, and this
  document claimed twice that it did.** `images.yml` last ran 2026-09-12, but
  that was the **ci-base job only**; the **zephyr job last actually ran
  2026-09-03** and has been `skipped` on every run since, because nothing
  under `ci/docker/zephyr-ros/**` moved. So the image did not change when the
  blackout began -- the REQUIREMENT did, and what started demanding `unzip`
  around 2026-09-11 is an open question this phase did not answer. The
  correlation that opened the phase is a coincidence in every row.
* `check-lane-contracts` has no reach gap. That was closed 2026-09-05 by
  issue 1030; it now reports 21 lane invocations, 3 merge-gating, none
  resolving an artifact its job does not build.

**The root cause of the two Zephyr-image rows is one drift.** `ci/docker/ci-base/Dockerfile`
and `ci/docker/zephyr-ros/Dockerfile` are both `FROM ros:humble-ros-base` with
hand-mirrored apt lists, and the Zephyr one lacks `unzip` and `python3-tomli`
that ci-base has. ci-base's own header says "the Zephyr CI image is FROM this
+ the Zephyr SDK layer" and zephyr-ros's header says it is `FROM
ros:humble-ros-base` directly, "despite" that -- the file already documents an
inheritance that does not exist, which is the bug's own confession.

`images.yml` last ran 2026-09-12, and the blackout begins 2026-09-11/12. A
Dockerfile edit changes nothing until that workflow republishes; issue 1201 is
the precedent, where `images.yml` failed silently and every lane ran
2026-08-29 content for days.

## Waves

One claim per wave. `owns` is advisory here rather than exhaustive -- these
are diagnosis waves, and what a wave ends up editing depends on what it finds.

| claim id | owns | starts now? |
| --- | --- | --- |
| `phase-466-W1` | LANDED (#1211). One shared `ci/docker/apt-packages.txt` both Dockerfiles COPY, so drift is UNREPRESENTABLE rather than detected; `check-ci-image-apt-packages`; the `setup-clang-format` fatality moved out of `_setup-common` into `check-tier-preconditions` | done |
| `phase-466-W2` | LANDED (#1209). Not `probe.yml` -- the narrow fix was already on main. The class gate `check-workflow-just-provisioning` plus the sweep of every workflow and composite action that invokes `just` | done |
| `phase-466-W3` | LANDED (#1210). THREE independent bugs: the missing `-DZEPHYR_EXTRA_MODULES`, the runner's absent rosidl python deps (1457, kept open -- not fixable in this repository), and a missing `mkdir -p` at `fixture-lane.sh:343` that silently lost issue 0499's lower bound | done |
| `phase-466-W4` | LANDED (#1212). Both lanes die in ONE step on a full disk (issue 1353). A survivable disk-report channel plus `scripts/ci/reclaim-disk.sh`, with `live-peer.yml`'s inline copy folded into it | done |
| `phase-466-W5` | OPEN (2026-10-14, handed over by the archived phase-330). The RTOS half of phase-330's two last acceptance boxes: build the RTOS realtime workspace fixtures and get a verdict from `realtime_tiers_e2e` and `sched_dims_applied_e2e` on them. See "W5 -- the RTOS realtime cells" below | waits on a tier-2 nightly that reaches its cells |

W1 and W3 may collide on the Zephyr image: if W3's `msg2idl.py` failure turns
out to be another missing package in that image, the Dockerfile edit belongs
to W1 and W3 reports rather than edits.

## Where the four waves left it (2026-09-23)

All four landed: #1209 (W2), #1210 (W3), #1211 (W1), #1212 (W4), plus this
document and its two corrections.

**The images republished themselves.** `images.yml` fired on the merge push
two seconds after #1211 landed (run 35843238854) and both jobs succeeded; the
published zephyr image's own build log echoes its shared apt closure with
`unzip` and `python3-tomli` in it. The "a maintainer must dispatch it" item
this document carried is RETRACTED -- the paths filter did its job.

**One cause remains outside this repository.** The tier-2 self-hosted runner
needs `catkin_pkg`, `empy==3.3.4` and `lark`. It has no `/opt/ros/humble`, so
the vendored rosidl clone is used correctly and its `[python.*]` deps are
report-only there; alternatively `nros setup --source rosidl` starts
provisioning them rather than reporting them. Until then the pairwise lane
stops in the same place -- now with a refusal naming the remedy instead of a
traceback eleven minutes into ninja.

**Nothing is verified yet, and will not be for hours.** No scheduled lane has
run since the merge. The first that will pull the new images are `live-peer`
(~04:15 UTC) and `nightly` (~05:10 UTC) on 2026-09-24. Both 1359 and 1364
stay `status: open` for exactly that reason: their acceptance is a scheduled
job reaching its cells, and that evidence does not exist.

**Still unattributed:** the tier-2 nightly job fails `provision-zenohd` with
exit 78 before reaching anything else. Nobody has claimed it.

## Issues homed here (2026-10-13)

| issue | why it belongs here |
| --- | --- |
| [#1651](../issues/archived/1651-host-tests-red-or-cancelled-on-most-pushes.md) | RESOLVED 2026-10-23: `host-tests` and tier 1 both went green on `main`; its later reds are other defects, each named in its resolution. |
| [#1878](../issues/archived/1878-metadata-probe-cmake-mirror-race.md) | RESOLVED as a duplicate of issue 1858 (`0fc6da85d6`). The cmake metadata probe's stale sizes-header mirror stopped tier 1 and tier 2 at `check fast` (`sizes-header-mirrors`) before any cell. The cause was a shared probe cargo dir whose key left out the late-applied `panic-platform` feature. |
| [#1915](../issues/archived/1915-cli-test-mutates-home-races-locate.md) | RESOLVED. Once tier 2 got past its build (run 38663782341), `check-cli-tests` was red on `board_facts`' agreeing-duplicates test. A `doctor` test set the process-wide `$HOME`, and the store root derives from it. So the two ladder walks on the runner, which holds the FreeRTOS kernel in the store and not as a submodule, resolved one kernel two ways. The store root is now a parameter, and the test no longer moves `$HOME`. The other red in that run, `template-copy-out`, is issue 1914. |
| [#1911](../issues/archived/1911-sizes-mirror-gate-judges-unbuilt-dirs.md) | RESOLVED. `sizes-header-mirrors` runs in `check fast` at the head of `just build`, before anything is rebuilt. In the persistent fixture tree it called a probe mirror stale when only a sibling build dir had rewritten their shared source after a core-size change. So both tiers of run 38637410239 stopped before the build. The mirror command now records each run, and the gate fails only a mirror whose last run postdates its source; the rest are reported as pending. |
| [#1905](../issues/archived/1905-gate-self-tests-write-job-env.md) | RESOLVED. `fixture-tree.sh --self-test`, reached by `check fast` inside `just build tier1`, appended a sandbox `NROS_FIXTURE_TREE` to the job's `$GITHUB_ENV`. Every step after the build then read a deleted temp tree, so tier 1 built everything and reached no cell (run 38615001083). The gate runner now unsets the job files, and both self-tests that reached a writer isolate themselves. |

## W5 -- the RTOS realtime cells (from phase-330, 2026-10-14)

Phase-330 (the SystemModel as a build artifact, RFC-0063) closed with its
native half measured: on 2026-10-14 a fresh worktree built every native
realtime row with `just build-test-fixtures lane=native`, and the native realtime
e2e family passed against generated models. That covered `native_orchestration_tiers`
(6/6), `realtime_subnode_cpp_e2e`, `realtime_subnode_cpp_portable_e2e`,
`sched_dims_model_coverage`, the 6 native rows of `realtime_tiers_e2e` and the
`linux rust CorePin` cell of `sched_dims_applied_e2e`.

Every other cell of those two tests was `[UNMET PRECONDITION] fixture not
built`. These are 14 of the 20 `realtime_tiers` rows and 16 of the 17
`sched_dims_applied` cells, on Zephyr, NuttX (arm and riscv), FreeRTOS and
ThreadX-linux. Those fixtures are built only by the scheduled tier-2 lanes,
which is this phase's subject, so their verdict is homed here:

- the fixture rows: `workspace-{rust,c,cpp}-nuttx-realtime`,
  `workspace-{rust,c,cpp}-nuttx-riscv-realtime`,
  `workspace-rust-threadx-linux-realtime`, `workspace-rust-freertos-realtime`
  (plus the FreeRTOS C/C++ entries the tests resolve), and the Zephyr west
  leaves `build-ws-{rs,c,cpp}-realtime-entry-zenoh`,
  `build-ws-cpp-derived-tiers-entry-zenoh`,
  `build-ws-rs-realtime-derived-entry-zenoh` and
  `build-ws-c-realtime-entry-smp`;
- the tests: `realtime_tiers_e2e` and `sched_dims_applied_e2e`.

Done when one scheduled run builds those rows and both tests report a verdict
on every RTOS cell: green, or red for a reason about the code under test. An
`UNMET PRECONDITION` is not a verdict. A red that is a real defect gets its own
issue; it does not reopen phase-330.

## Two things this phase must not do

**Do not fix only where the symptom was seen.** The package absence is not the
interesting defect; the interesting defect is that two package lists must
agree and nothing makes them agree, and that setting up a *Zephyr workspace*
fails because a *code formatter* is missing. One absent package turned into 22
dead jobs through a coupling nobody chose.

**Do not report a lane as fixed because its first failure is fixed.** A lane
reports only its first failure, which is how issue 1070 stayed invisible
behind issue 1025 for as long as it did. After seventeen days of tier 2 never
reaching a cell, a second fault behind the first should be the expectation,
not the surprise.

## Acceptance

Per lane: one scheduled run that reaches its own work and reports a verdict --
green, or red for a reason that is about the code under test rather than about
the lane. Not "the step passes": `run-matrix` and `nightly` tier 2 must reach
a cell, `probe` must run a book block, `host-tests` must run tests.

The phase closes when every row of the measurement table above has a success
in its window, or carries a written reason why it cannot.

## Issues homed here (2026-10-17)

Homed when every open issue was given a phase. Each issue has a `**Home:**` line pointing here.

| issue | why it belongs here |
| --- | --- |
| [#1136](../issues/archived/1136-launch-args-keyword-removed-but-still-generated.md) | `host-tests`' integration job died at **Build workspace fixtures** for 30 runs because the CLI still emits the removed `LAUNCH_ARGS`. That is a scheduled lane with no verdict. |
| [#1325](../issues/archived/1325-zephyr-fixture-filter-matches-the-checkout-path.md) | RESOLVED 2026-10-22 — the filter now matches per field, repo-relative, in both Zephyr builders (gate `check fixture-filter-fields`). |
| [#1355](../issues/archived/1355-threadx-riscv64-builds-the-linux-threadx-port.md) | RESOLVED 2026-10-22: the port defect was long gone; the lane then went green without running its cells, because `just threadx_riscv64 test` never selected `rtos_e2e`. It does now, gated by `check platform-lane-cells`. |
| [#1365](../issues/1365-tier2-self-hosted-runner-unregistered.md) | Tier-2 jobs wait for a self-hosted runner that is not registered, and the lane reports nothing. |
| [#1375](../issues/archived/1375-zephyr-native-sim-runner-link-misses-zpico-reply-slot.md) | RESOLVED 2026-10-23: Zephyr Rust zenoh images link, boot and pass in the nightly (39739466151). A merge-gating Zephyr link was declined; the nightly is that lane. |
| [#1459](../issues/archived/1459-threadx-c-entry-undefined-network-wait.md) | RESOLVED 2026-10-23: the weak-reference fix holds, and the nightly `threadx_linux` job builds and passes the C images. A C-only image in L3 was declined; the nightly is that lane. |
| [#1474](../issues/archived/1474-zephyr-rust-fixture-build-has-no-clippy-component.md) | The Zephyr Rust fixture build runs clippy as a ninja step, and the CI image has no clippy component, so `live-peer` dies in its build. |
| [#1483](../issues/1483-python-layer-carries-upstream-transitive-deps.md) | `[python.*]` lists two of upstream rosidl's transitive dependencies. It came from the CI-image fix of issue 1457, which is homed here. |
| [#1493](../issues/archived/1493-heap-free-poc-bin-has-no-row-and-no-lane-declaration.md) | `fixture_source_coverage` was red on main for 27 days because no lane runs that test. |
| [#1497](../issues/archived/1497-tier2-zephyr-self-pkg-leaves-have-no-systemmodel.md) | Tier 2's Zephyr module dies in `codegen-system`, with no cell verdict. The root cause, issue 1501, is fixed; what is left is a scheduled run that reaches the cells. |
| [#1579](../issues/archived/1579-timer-elapsed-upper-bound-fails-under-ci-load.md) | A real sleep bounded by a 10 ms allowance fails `timer_readiness_and_elapsed_agree_with_the_dispatcher` on a loaded runner, in tier 1. |
| [#1590](../issues/archived/1590-threadx-rv64-rust-leaf-misses-cyclone-backend-archive.md) | A `threadx_riscv64` Rust leaf built with Cyclone linked no Cyclone backend archive. Fixed by `be2d7efd39`; 12/12 leaves link (2026-10-21). |
| [#1592](../issues/archived/1592-features-custom-msgs-declares-rosidl-default-deps.md) | In-tree `package.xml` names resolved only through ament, so the ROS-less tier-2 runner refused them. The sites were fixed in October; a no-ROS ladder test now catches the class. |
| [#1628](../issues/archived/1628-tier1-rust-rtos-link-check-needs-unprovisioned-freertos.md) | `just ci tier1` runs `rust-rtos-link-check`, which needs a FreeRTOS kernel that `host-tests.yml` never checks out. |
| [#1665](../issues/archived/1665-zephyr-ws-rust-leaf-cannot-find-core.md) | Zephyr workspace Rust entries fail with `can't find crate for core` in the tier-2 nightly, in 10 of 32 images. |
| [#1666](../issues/archived/1666-zephyr-ci-image-has-no-cargo-nextest.md) | live-peer's board job reaches its run step, and the Zephyr CI image has no `cargo nextest`. |
| [#1755](../issues/archived/1755-runner-home-cache-not-writable.md) | After a runner-container restart, two tier-1 steps cannot create directories under `~/.cache`. |
| [#1758](../issues/archived/1758-skip-and-pass-is-a-failure.md) | RESOLVED 2026-10-23: scope, not host probes, decides what runs. Every scheduled lane that gates on it now reaches a verdict or stops on a different, tracked defect. |
| [#1842](../issues/1842-tier-priority-plan-image-red-verdict-unprinted.md) | `tier-priority-plan-image` fails the Zephyr fixture build in two scheduled lanes, and neither lane shows its verdict. |
| [#1898](../issues/archived/1898-metadata-harness-excerpt-drops-cargo-cause.md) | The tier-2 nightly stopped in `generate-bindings` on a git dependency it could not fetch, and the excerpt dropped the reason. |
| [#1899](../issues/1899-tier2-nightly-fixture-compilers-sigkilled.md) | Compilers were SIGKILLed across five families at once in the tier-2 nightly build, so crates fail with no diagnostic. |
| [#1981](../issues/1981-live-peer-narrowing-alternation-is-one-workspace-id.md) | Homed 2026-10-23 from phase-441, which was archived that day. The `live-peer` board job joins two `NROS_FIXTURE_ID` narrowings with `\|`, and the workspace builder reads the result as one id. So the lane cannot run the fragmented-sample cell phase-441 W6 recorded. |
