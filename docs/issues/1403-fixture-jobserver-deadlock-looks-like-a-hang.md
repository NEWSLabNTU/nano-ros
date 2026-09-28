---
id: 1403
title: "`build-examples` deadlocked on its own make jobserver FIFO with zero
  children left — a CI lane hitting this reads as a HANG, not a failure"
status: open
type: bug
area: [build, ci]
severity: medium
found: 2026-09-21
related: [0022, 0616, 1390]
---

## What was observed

During issue 1390's measurement, `just threadx_linux build-examples` with the
default `NROS_BUILD_JOBS` (20 on this host) **deadlocked**: GNU make 4.4.1
blocked in `pipe_read` on its own `--jobserver-style=fifo` FIFO, with **zero
children** and nothing left to run, after all 14 fixture rows had started.

Killed and re-run with `NROS_BUILD_JOBS=1`; that build and a subsequent one both
completed cleanly (rc=0).

A self-hosted CI runner container was also building on this host at the time.

## Why it is worth filing rather than shrugging at

The failure mode is the dangerous one. A lane that deadlocks produces **no
verdict**: it looks like a slow build until a timeout kills it, and a timeout
kill reads as infrastructure flake rather than as this. CLAUDE.md already records
the more general form — *"a red CI lane answers one of two questions and they
look identical"* — and a HANG is the degenerate case where the lane never
answers at all.

`build-examples` is reached by `build-all` on the schedule, so this sits on a
lane nobody watches closely.

## What is NOT established

Almost everything about the cause, and this issue does not claim otherwise:

* **Not reproduced deliberately.** One occurrence, worked around, not bisected.
* Whether the concurrent CI-runner container is necessary, contributory, or
  irrelevant. Two jobserver-using builds on one host is the obvious suspect and
  is entirely unproven.
* Whether it is `NROS_BUILD_JOBS=20` specifically, or any value above some
  threshold, or a race that 20 merely makes likely.
* Whether this is issue 0022's family one level up. 0022 is recursive cargo
  under the fixture jobserver, fixed by stripping `MAKEFLAGS`/`CARGO_MAKEFLAGS`
  from the nested probe cargo — the shape rhymes (a child waiting for a token a
  parent holds), but zero children and a blocked parent is not the same picture,
  and calling it 0022 without measuring would aim the next person at a fix that
  is already in place.

## Direction

The cheap first step is evidence capture rather than a fix: when
`fixture-make-driver.sh`'s make blocks, record `/proc/<pid>/stack` or a `gdb`
backtrace plus the FIFO's state, and whether any other jobserver-holding process
is live on the host. A deadlock that leaves a diagnostic behind is a bug someone
can take; one that leaves a timeout is not.

Worth pairing with a watchdog on the lane so the schedule reports "no verdict"
explicitly instead of a timeout — the distinction `just nightly-triage` already
makes for red-vs-never-ran.

Observed and reported during issue 1390's work; the observer explicitly did not
chase it. Recorded here rather than lost, with the caveats intact.

## Evidence capture (landed)

The cause is still unknown and this issue stays open. What changed is that a
repeat can no longer pass as a hang or a timeout.

Every fixture/jobserver `make` — `fixtures-build.sh`'s `run_with_make` (the
make that stalled here: 14 `threadx-linux rust` rows), both fixture make
drivers, `workspace-fixtures-build.sh`, `jobserver-pool.sh` and
`build-all-jobserver.sh` — now runs under
`scripts/build/make-stall-watchdog.py`. When make has had **no live child and
burned no CPU** for `NROS_JOBSERVER_STALL_SECS` (default 600 s, sampled every
`NROS_JOBSERVER_STALL_POLL_SECS` = 15 s; `0` disables), it:

1. writes `jobserver-stall-<pid>-<time>.txt` into that build's log dir and to
   stderr: `/proc/<pid>/stack` (root-only; the reason is recorded when it is
   refused) plus `wchan`, `syscall` and the signal-mask lines of `status`;
   make's fds; the jobserver it inherited (`MAKEFLAGS`/argv) and the FIFO/pipe
   fds it holds with each one's FIONREAD byte count — for `-jN` an idle
   top-level make should see `N-1` token bytes, so a shortfall says a token is
   held or lost; every other process on the host with a jobserver in its argv
   or environ or holding one of make's FIFO fds, marked in-tree or NOT, and how
   many processes could not be read; and a `gdb` backtrace when
   `kernel.yama.ptrace_scope` is 0 (otherwise the reason it was skipped — never
   escalated);
2. kills make and its descendants, found by walking parent pids down from the
   one pid it started — never by name, and not by process group, because under
   `subtree-guard.sh` make shares the outermost launcher's group;
3. exits **75** with `NO VERDICT: jobserver stall — <file>`. `just
   nightly-triage` reads that line from a failed job's log and counts the job
   as NO VERDICT, not as a failure of the code.

Guarded by `just check make-stall-watchdog` (fast line): the real watchdog
against a fake make blocked reading an empty FIFO with an outside process also
holding it (must fire, report both, kill only the fake), plus busy, failing,
quick and disabled fakes that must pass through untouched.

**If you get a dump:** attach the file here. The lines that discriminate
between the hypotheses above are the FIONREAD count against `N-1`, whether any
process NOT in make's tree holds make's FIFO (the concurrent-build suspect),
whether zombie children are listed (make stopped reaping), and the SigBlk/SigIgn
masks. Do not raise the threshold to make it go away; a dump is the point.
