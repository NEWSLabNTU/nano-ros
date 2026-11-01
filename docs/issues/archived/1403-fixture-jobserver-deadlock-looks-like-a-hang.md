---
id: 1403
title: "`build-examples` deadlocked on its own make jobserver FIFO with zero
  children left — a CI lane hitting this reads as a HANG, not a failure"
status: resolved
type: bug
area: [build, ci]
severity: medium
found: 2026-09-21
related: [0022, 0616, 1390, phase-505]
resolved_in: "phase-505 W3: detected as a failure; every parallel make is under the stall watchdog, gated by `check-jobserver-makes-watched`"
---

**Home: [phase-505](../../roadmap/archived/phase-505-ci-lanes-reach-and-infra-robustness.md)** — homed 2026-10-17; listed in that phase's "Issues homed here (2026-10-17)".

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

## Resolution (2026-10-19, phase-505 W3): detected as a failure

Phase-505's item reads "the jobserver deadlock is fixed, **or it is detected as
a failure**". The cause is still unknown and has still not been reproduced.
What is closed is the failure mode that made this worth filing: a stall can no
longer be a silent hang that a CI timeout reports as an infrastructure flake.

**The sweep found the watchdog reached fewer makes than claimed.** "Every
fixture/jobserver `make`" above was not true. Four `make -j` invocations ran
outside the watchdog:

- `build-test-fixtures`' top-level fan-out (`justfile`). Every fixture build
  shares this make's jobserver.
- `linux build-examples`' fan-out (`just/linux.just`).
- The two NuttX kernel makes (`scripts/nuttx/build-nuttx.sh`,
  `just/nuttx.just`). These matter because they run INSIDE fixture builds: a
  stalled inner make keeps the outer make's child alive, so no outer watchdog
  can see the stall.

All four are wrapped now.

**Gate: `just check jobserver-makes-watched`** (fast line, static). In
`justfile`, `just/*.just` and `scripts/*.sh`, every shell command that runs
`make` with `-j`, `--jobserver-style` or a `make_args` array must have
`make-stall-watchdog.py` in the same logical command. Continuation lines are
joined, and `echo`/`printf` lines count as mentions, not invocations. Its
self-test runs on every invocation. Run against the tree before this change, it
reports exactly the four sites above. It scans 348 files and finds 0 now.

**The watchdog itself can no longer hang** (issues
[1773](1773-stall-watchdog-host-scan-blocks-on-hung-fuse.md) and
[1785](1785-stall-watchdog-diagnostics-hang-on-fuse-fd.md)). The whole
diagnostics phase runs under a deadline, so the kill and the `NO VERDICT`
(rc 75) always happen, and `just nightly-triage` already counts that line as
NO VERDICT.

**If it happens again**, the dump is the next step, as "If you get a dump"
above says. File a new issue with it rather than reopening this one, because
the open question is now the cause, not the detection.
