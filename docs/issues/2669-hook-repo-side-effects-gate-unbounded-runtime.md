---
id: 2669
title: "`check-hook-repo-side-effects` takes 98 s to over 1,900 s on the fast line, and its per-script probes have no time limit, so it can cancel any merge group"
status: open
type: bug
area: [ci, testing]
severity: medium
found: 2026-11-17
related: [0986, 1465, 2240, 2625, phase-559]
---

**Home:** [phase-559](../roadmap/phase-559-ci-workflow-organization.md),
"Issues homed here". The gate is on the fast line, which every lane runs.

## What happens

On 2026-11-17 the merge group for #4125 (run 44022704628) was ejected with
`check / fast (every event)` CANCELLED after 40 minutes. `just check fast`
normally takes about 5 minutes on that job. 598 of its 599 gates had passed
by 05:35:49. The one still running was `fast::check::hook-repo-side-effects`,
reported SLOW every 120 s from about 05:21:30:

    SLOW [>1920.000s] (───────) nros-gates::gates fast::check::hook-repo-side-effects
    ...
    ##[error]The operation was canceled.   (05:54:53)

The change in that pull request adds no script, hook or `git init` path: it
is trace-marker code in C, C++ and Rust, docs, and four API-parity ledger
rows. Nothing in it is a subject of this gate.

## The same gate elsewhere, the same day

| Where | Load | Time |
| --- | --- | --- |
| merge group for #4123, run 44016560135 (passed) | hosted runner | 682 s |
| local `just check fast`, 2647 branch | load average about 16 | 98 s |
| local `just check fast`, #4009 branch | load average about 27 | 1,084 s |
| merge group for #4125, run 44022704628 (cancelled) | hosted runner | over 1,920 s |

That is a spread of more than 10x for the same gate on the same day. On the
fast line, a gate whose time depends on host load this much can push any
pull request's merge group past its job budget, whatever the pull request
changes.

## Why it is unbounded

`scripts/check-hook-repo-side-effects.sh` discovers every tracked script that
builds a repository (`hazardous`, from `git ls-files 'scripts/**' .githooks/*`
filtered on `git init`). It runs each one under `run_probe`: twice in the
loop for the `worktree` and `explicit` shapes, and once more in the
concurrent `tmpdir_pass` (issue 2240). Each probe builds a victim repository
and, for `@checkout`, a `--local --shared` clone of this checkout (issue 1465).
It then runs the script.

Only the end-to-end hook run has a bound (`timeout 180 bash "$REPO/$HOOK"`).
The per-script probes have none. So one probe that blocks, or many that slow
down together under load, stretch the whole gate. The gate's nextest profile
reports SLOW but never terminates it, so the job's own timeout is what ends
it, as a cancellation that names no probe.

Not measured yet: which probe was running when run 44022704628 stalled, and
whether it was one blocked probe or the whole set slowing down. The job log
shows the gate as SLOW but not which subject it was on.

## What would close it

- A per-probe timeout that FAILS the gate and names the probe and its shape,
  so a stall is a red with a name instead of a cancelled job.
- A measurement of why a probe is slow under load: per-probe wall time
  printed with its verdict, so the next slow run says where the time went.
  The victim and owned-checkout builds, about 3 s each according to the
  `TMPDIR_PROBE_REUSE` comment, are the first suspects, multiplied by
  scripts times shapes.
