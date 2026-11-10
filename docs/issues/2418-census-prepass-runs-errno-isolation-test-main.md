---
id: 2418
title: "The census pre-pass runs the ThreadX `errno-isolation` fixture's own test `main` on the host, where its observer thread fails to spawn, and stops the tier-2 nightly before any fixture builds"
status: open
type: bug
severity: high
area: [census, ci, threadx, fixtures]
related: [2390, 2396, 2137, 0680, 2335]
found: 2026-11-10
---

## Measured

Scheduled `nightly` run 42760153000 on `main` `20d55a6101`, job 129643681528
"tier 2 nightly (pairwise cover)":

```
census-prepass: 51 leaf candidate(s), 48 at a time
census-prepass: 1 leaf census(es) FAILED:
  - examples/rv-virt-threadx/c/errno-isolation (exit 1; ...), last lines:
      | errno-isolation: start
      | errno-isolation: task storage need=8 have=512
      | errno-isolation: verdict SETUP failed (observer)
      | Error: census run of `.../errno-isolation/build/census-host/c_errno_isolation` exited exit status: 1 and wrote nothing -- the funnel prints the reason
error: recipe `build-test-fixtures-leaves` failed with exit code 1
```

The job stops there; no fixture stage runs.

## What it is

`examples/rv-virt-threadx/c/errno-isolation` is a ThreadX regression witness
for issues 0680 and 2335 (per-thread newlib `errno`). Its `system.toml`
declares one `[[component]]`, so the pre-pass treats it as a leaf whose
entity census must be taken. It builds the leaf for the host
(`census-host`) and runs it.

The leaf, though, writes its own `main` (phase-499 W5). That `main` is the
test, not a node. On the host it calls `nros_platform_task_init(observer_task,
&attr, ...)` with `attr.priority = NROS_PLATFORM_PRIORITY_MAX / 4`, which
fails on the POSIX port in the CI container (`main.c:146`, `MARK_SETUP
"(observer)"`). The program returns 1 having created no node, so the census
records nothing and the pre-pass treats that as fatal.

The probable reason the POSIX spawn fails is that a real-time priority needs
privileges the container does not grant. Not measured.

## Why it shows up now

The pre-pass is new on this lane. The 2026-11-09 07:08 nightly's tier-2 job
(job 129028272568) has no `census-prepass` line at all; that job failed later,
at `build-test-fixtures: FAILED stage(s): zephyr(rc=2) freertos(rc=2)`. The
pre-pass reached this lane through phase-530's census work on 2026-11-09 and
2026-11-10 (`ebe1658f19`, `7bcde5f1ec` for issue 2390, `50e1b4d766` for issue
2396). So this is the first night it ran this leaf. The tier-2 lane (not
nightly) does not carry this leaf: the 118 worker's clean `lane=tier2` build
on the same commit passed its pre-pass.

## What it is NOT

- Not issue 2390 (Zephyr west leaves skipping the pre-pass). This leaf is a
  cmake ThreadX leaf, and it was taken.
- Not a ThreadX defect: the failure is on the host build, which never runs
  ThreadX.

## What would close it

The nightly's pre-pass passes this leaf. Possible fixes, which disagree on
where the rule lives:

1. A leaf whose `main` is application-written (no generated entry) gets no
   host census run, because its `main` is not a node and running it proves
   nothing about entities. Its census is then whatever its `system.toml`
   declares.
2. The witness stops declaring a `[[component]]`, since it registers no ROS
   entity at all.
3. The census-mode host build returns 0 before the test body. This is the
   weakest option: it teaches one witness about the census instead of fixing
   the class.

Whatever the fix, sweep for other application-`main` leaves with a
`[[component]]` that the pre-pass would also run.
