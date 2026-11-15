---
id: 2602
title: "`just ci gate` has no host lock: \"one gate per host\" is a convention, and two concurrent gates turn each other red"
status: resolved
type: bug
severity: medium
area: [ci, gates, testing]
related: [phase-489, 2380]
found: 2026-11-15
resolved: 2026-11-15
resolved_in: "the change that filed it"
---

**Home:** [phase-489](../../roadmap/phase-489-ci-build-speedup.md). Two gates on one
host slow each other into red, which costs the local tier its signal. That
reliability belongs to the phase that owns the gate's cost.

## What

"One gate per host" is agent convention only. Nothing stops a second
`just ci gate` from starting beside a first. On 2026-11-15:

- On newslab-243, `check::cli-tests` timed out while two full gates ran.
- On newslab-175, at a load average of about 50, both gates went red, and the
  branch was then pushed on partial reruns.

A red caused by host contention reads exactly like one caused by the code, so the
lane either wastes a run or teaches a rerun habit.

## Design (user-approved, relayed by the nros 241 manager, 2026-11-15)

1. `just ci gate` runs its body under
   `flock -o -w <cap> <lock> just _ci-gate-inner`. The `flock` process holds
   the lock, and `-o` keeps the descriptor out of its children, so an orphaned
   cargo or QEMU cannot keep the lock after `flock` dies. The kernel releases
   it on any death (SIGKILL, OOM, reboot). There is no pidfile and no
   "file exists" logic.
2. One lock per host, on a local filesystem: `$XDG_RUNTIME_DIR/nros-gate.lock`,
   falling back to `~/.cache/nros/gate.lock`. Never per worktree, never on
   NFS.
3. The holder writes its PID, worktree, branch and start time into the lock
   file. A waiter prints that holder line every few minutes and checks it with
   `kill -0`.
4. An overall gate timeout (about 2.5 h): on expiry the gate kills its own
   process group and exits red, "gate timed out", so a hung holder frees the
   lock itself.
5. The waiter's cap (about 3 h) fails red naming the holder ("wait, or ask the
   user"). It never proceeds without the lock.
6. No instruction anywhere to delete the lock, and no environment knob to skip
   it, since either would become the bypass.

Tests, each with a negative control:

- a second gate waits, then runs;
- `kill -9` of the holder lets the waiter in within about 1 s;
- an orphaned child does not hold the lock;
- a hung holder times out, and the waiter gets in;
- the waiter's cap expires red, naming the holder.

## Not in scope

Fixture builds outside the gate (`just build-test-fixtures`, `just ci tier1`)
contend for the same host. They could take this lock later, but this issue covers
`just ci gate` only.

## Resolution (2026-11-15)

`just ci gate` is now `bash scripts/ci/gate-host-lock.sh just ci::_gate-inner`,
and the old body is the private `_gate-inner`. The script follows the design
above point by point:

- `flock -o -E 75 -w <cap>`. Exit 75 is the waiter's expiry; a gate's own 75
  is mapped to 1, so the two never read alike.
- The holder line: `pid= worktree= branch= started=`, written by the held side
  and cleared on exit.
- the repo's deadline spelling (`NROS_DEADLINE`: `timeout`, then KILL after 3 s),
  not `--foreground`, so expiry signals the gate's
  whole process group. It exits red "gate timed out".
- A waiter's report every 300 s, with `kill -0`.
- A lock dir on a network filesystem is refused.

The durations can be shortened only through test-only `_NROS_GATE_LOCK_TEST_*`
variables, and none of them runs a gate unlocked: a zero wait is an immediate
refusal, and that is tested.

`tests/gate-host-lock-tests.sh` (`just check gate-host-lock`, fast line, about
15 s) has six cases, each with a negative control:

| case | negative control |
| --- | --- |
| a second gate waits, then runs | two lock dirs overlap |
| `kill -9` of the holder lets the waiter in within about 1 s | the waiter is still out while the holder lives |
| an orphaned child holds nothing | a bare `flock` without `-o` passes the fd to the orphan, which keeps the lock |
| a hung holder times out red, the waiter gets in, and the process group is gone | the waiter is out before the timeout |
| the waiter's cap is red, names the holder and never runs | a longer cap runs |
| a zero wait refuses | — |

Mutation-tested: dropping `-o` fails the kill -9 and orphan cases, and dropping
`timeout` fails the hung-holder case.

`check-default-gates-run-somewhere` reads the lane's `steps=(…)` through a
module-qualified private delegate now (`just ci::_x`), the same rule
`nros-tests`' `ci_lane` applies.

One property to know: `$XDG_RUNTIME_DIR` is `/run/user/<uid>`, so the lock is
one per USER per host. Every agent here runs as the same user, so the lock
holds across them. A gate run under another uid on the same host takes its
own lock.

## Follow-up: the wall limit scales, and a hang detector (2026-11-15)

nros wtf measured a normal full gate on a 4-core shared host at about 2.5 h
(`check::build` alone took 2h05m), which is right at the original limit. User
ruling, the same day:

- The **wall limit** is 2.5 h × max(1, 16 / nproc): 2.5 h at 16 or more cores,
  5 h at 8, 10 h at 4. The **waiter cap** is the wall limit + 30 min, so a
  waiter outlasts any live holder. Both are printed when a gate starts and
  while a waiter waits.
- A **hang detector**: a holder that makes no progress for 45 min is killed
  (its whole process tree) and exits red "gate hung: no progress for 45 min",
  which frees the lock. Progress means EITHER new gate output (the gate's
  output goes through a FIFO into `tee`, and the detector watches the copy
  grow) OR growth in the CPU time of the gate's process tree. That time is
  utime+stime plus reaped children's cutime+cstime, summed over every
  descendant, so nextest's per-test process groups count. A long quiet
  compile therefore lives, and a 0 % CPU deadlock does not. nros 175's
  nested-cargo deadlock held a host for 3 h this way.
- None of these is a user knob. The tests shorten them through
  `_NROS_GATE_LOCK_TEST_*`.

Tests added, with negative controls:
- a silent gate that burns CPU outlives the hang limit;
- a silent gate with no CPU is killed red as hung, frees the lock, and its
  tree is gone;
- a gate that prints while idle is alive;
- the wall limit and waiter cap at 4, 8, 16 and 48 faked cores.

Mutation-tested: dropping the CPU half of "progress" kills the busy gate, and
dropping the kill lets the idle one live.
