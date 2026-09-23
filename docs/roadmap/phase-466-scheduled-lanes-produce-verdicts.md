# phase-466 -- every scheduled lane produces a verdict again

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
[1158](../issues/1158-tier2-lane-has-produced-no-verdict-for-six-days.md) (tier 2, filed
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
  their failures begin **2026-09-03**, before the 2026-09-12 image build. The
  timing correlation that opened this phase is a coincidence for these two
  lanes. It still holds for the Zephyr ones.
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
| `phase-466-W1` | `ci/docker/ci-base/Dockerfile`, `ci/docker/zephyr-ros/Dockerfile`, whatever gate stops the two lists drifting, the `_setup-common` -> `setup-clang-format` coupling | yes |
| `phase-466-W2` | LANDED (#1209). Not `probe.yml` -- the narrow fix was already on main. The class gate `check-workflow-just-provisioning` plus the sweep of every workflow and composite action that invokes `just` | done |
| `phase-466-W3` | LANDED (#1210). THREE independent bugs: the missing `-DZEPHYR_EXTRA_MODULES`, the runner's absent rosidl python deps (1457, kept open -- not fixable in this repository), and a missing `mkdir -p` at `fixture-lane.sh:343` that silently lost issue 0499's lower bound | done |
| `phase-466-W4` | LANDED (#1212). Both lanes die in ONE step on a full disk (issue 1353). A survivable disk-report channel plus `scripts/ci/reclaim-disk.sh`, with `live-peer.yml`'s inline copy folded into it | done |

W1 and W3 may collide on the Zephyr image: if W3's `msg2idl.py` failure turns
out to be another missing package in that image, the Dockerfile edit belongs
to W1 and W3 reports rather than edits.

## What still needs someone with access

Two of the six causes cannot be closed from inside this repository:

1. **The tier-2 runner needs `catkin_pkg`, `empy==3.3.4` and `lark`.** It has
   no `/opt/ros/humble`, so the vendored rosidl clone is used correctly and
   its `[python.*]` deps are report-only there. Alternatively `nros setup
   --source rosidl` starts provisioning them rather than reporting them.
   Until then the pairwise lane stops in the same place -- now with a refusal
   naming the remedy instead of a traceback eleven minutes into ninja.
2. **`images.yml` must republish** before any Dockerfile change reaches a
   lane. Issue 1201 is the precedent.

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
