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
- A waiter's report every `NROS_GATE_LOCK_REPORT_S` (300 s), with `kill -0`.
- A lock dir on a network filesystem is refused.

The durations can be shortened through `NROS_GATE_TIMEOUT_S` and
`NROS_GATE_LOCK_WAIT_S`, but none of them runs a gate unlocked: a zero wait is
an immediate refusal, and that is tested.

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
