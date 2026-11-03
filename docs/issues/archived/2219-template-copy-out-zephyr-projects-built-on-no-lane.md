---
id: 2219
title: "template-copy-out's five Zephyr projects are built copied out on no lane, and gate.yml still cites a resolved issue as the reason"
status: resolved
resolved_in: "2026-11-04"
type: tech-debt
area: [ci, templates, zephyr]
found: 2026-11-03
related: [issue-1453, issue-1476, issue-1782, issue-2140, phase-489, rfc-0117]
---

## The gap

Issue 1453 put `just check template-copy-out` on the merge queue so that every
copy-out template is built copied out before it merges. Five of its projects
are never built on any lane:

- `ament-workspace-port/zephyr`
- `cpp-port-minimal-publisher/zephyr`
- `rclcpp-compat-smoke/zephyr`
- `topic-state-monitor-port/zephyr`
- `zephyr-byo/app`

In all 20 successful `merge_group` runs of `gate.yml` on 2026-11-02
(41381715200 … 41496872163), each of the five took about 1 s and was reported
as follows:

```
[SKIPPED] template-copy-out: NOT VERIFIED — <project> (<board>): Zephyr not provisioned — no west workspace resolves (just zephyr setup)
```

The `check-template-copy-out` job provisions the FreeRTOS cross toolchain and
sources, but no west workspace. The other lane that runs the gate is
`host-tests.yml`'s `tier-1 gates` shard (hosted `ubuntu-22.04`, in the CI
image; it ran the gate in run 41381866329, 3445 s). That job has no Zephyr
setup step either. Its parallel runner prints no per-gate output, so the
skip lines do not reach the job log. As far as the workflows show, none of
these five has been built copied out in CI.

It is not silent in the log. Each skip goes through `nros_check_unverified`
into the `nros_check_skip` ledger, and the gate's closing line points at it.
But the lane's green does not mean what issue 1453 says it means, and no lane
claims these five as out of scope.

## Why it reads as deliberate when it is not

`gate.yml`'s comment on `check-template-copy-out` still says that "the Zephyr
ones have no copy-out road the gate can derive (issue 1782)", and the
provisioning step repeats it. Issue 1782 was resolved on 2026-10-29 by giving
the gate exactly that road: `nros configure` + `nros build` in the copied leaf,
against the resolved west workspace. Its acceptance was measured locally with
Zephyr installed. The rationale in the workflow is therefore stale. The only
thing still missing is the precondition.

## Options

1. Provision a Zephyr 3.7 west workspace on a lane that runs the gate (the
   cost of `nros setup zephyr` on a hosted runner: west update, SDK, disk —
   issue 1353), and keep these five NOT VERIFIED on the merge queue as a
   stated scope decision.
2. Provision it in the merge-queue job itself, or in one shard of it
   (phase-489 W8 shards the job), so that issue 1453's guarantee covers Zephyr.
3. Make the scope explicit: declare in the lane that it does not claim the
   Zephyr road, and name the lane that does.

Whichever is chosen, the workflow comment must stop citing 1782 as the reason.

## Acceptance

- Some named lane builds all five Zephyr projects copied out, with the log
  showing `<project>: OK`. Or the merge queue does, and its job log shows them
  OK.
- `gate.yml`'s comment states the real scope.

## Resolution (2026-11-04)

Option 2. The merge queue builds all five projects. `template-copy-out` now
has a fourth merge-group part, `check / template-copy-out [zephyr]`.

- **The part split.** `check-template-copy-out.sh --part zephyr|no-zephyr`
  keeps one of two complementary parts. A unit's part comes from `cross_road`,
  the predicate `build_cross` dispatches on, so a new Zephyr template joins
  without a list. The three shards run `no-zephyr`. The new job runs `zephyr`
  in `nano-ros-zephyr-ci` on a Zephyr 3.7 west workspace, with
  `NROS_CHECK_SKIP_STRICT=1`. It installs the index's pinned SDK (0.16.8),
  not the image's 0.17.4, which fails 3.7 on Arm (issue 1811).
  `check-merge-group-gates` rule 6 refuses a part with no complement. A run
  that has units in scope and builds none of them now fails. It used to
  print "OK — 0 project(s) built". There is a self-test for this.
- **A jobserver defect.** The first dispatch after the rebase failed
  `zephyr-byo/app` at native_sim's `zephyr.exe` link with `make: *** internal
  error: invalid --jobserver-auth string 'fifo:…'`. RFC-0117 D4's
  `choose_style` chose the pipe form only for a CMake Makefiles build. But a
  west/Ninja build reaches make too: native_sim's runner runs the image's
  make 4.3, and so can a CMake ExternalProject or a cargo build script.
  `choose_style` now takes only the version of the make on PATH, for every
  driver.
- **Not cached, by measurement.** The 0.16.8 SDK is 7.8 G unpacked (1.4 GB
  fetched). The 3.7 west workspace is 2.5 G. The repository's Actions cache
  held 9.18 GB of its 10 GB budget (CLI and sccache entries the other lanes
  restore). An entry this size would evict those on every merge group. So
  west fetches the pinned revisions and the SDK is the pin on every run, and
  no restored tree can stand in for either (issue 1476's trap does not
  arise). What was cut instead: the XRCE Agent build (a runtime peer, about
  30 s) and the disk reclaim (36 s; 61 G was free). The part uses the CLI
  binary cache the shards already share.

`workflow_dispatch` runs on `issue-2219-zephyr-copy-out-v2`. All five
projects were OK in both:

| run | CLI cache | [zephyr] | init / CLI / Zephyr setup / build, s | shards 1/2/3 |
| --- | --- | --- | --- | --- |
| 41619834234 (before the rebase and the trims) | miss | 26.7 min | 262 / 236 / 425 / 610 | 20.0 / 18.0 / 12.9 min |
| 41820109990 (CLI sources changed) | miss | 26.2 min | 265 / 258 / 400 / 622 | 13.9 / 18.2 / 18.6 min |
| 41824085384 | hit | 18.5 min | 200 / 45 / 240 / 599 | 15.3 / 13.3 / 10.6 min |

With a hit, the Zephyr part is not the gate's critical path. A merge group
whose pull request changes CLI sources misses the CLI cache, and then this
part is about 26 min, which is also true of the shards' CLI step.
