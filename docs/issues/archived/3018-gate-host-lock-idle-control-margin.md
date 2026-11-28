---
id: 3018
title: "gate-host-lock's live-gate controls run a 2 s hang limit, so a descheduled tick under a loaded gate reads as hung"
status: resolved
type: bug
severity: low
area: [ci, testing]
related: [issue-2602, issue-2872]
found: 2026-11-28
---

## What happens

`tests/gate-host-lock-tests.sh` has two cases that assert a LIVE gate is not
killed by the hang detector in `scripts/ci/gate-host-lock.sh`:

- case 7, "a silent gate that burns CPU outlives the hang limit" (a 5 s CPU
  loop, no output);
- case 8's negative control, "a gate that prints while idle is alive" (an
  `echo tick` every 0.5 s for 5 s).

Both ran with `_NROS_GATE_LOCK_TEST_HANG_S=2` and a 1 s detector tick. The
detector counts a tick as idle when the gate's log did not grow and its process
tree's CPU time did not move, and kills the gate after `HANG_S` seconds of
idle ticks. So each control had to show progress within every 2 s window. On a
loaded host a 0.5 s `sleep` fork, or the `tee` that carries the tick into the
log, can be descheduled for longer than that, and the live gate is killed as
hung.

Measured 2026-11-28 in a full `just ci gate` of PR #5018 at 6e99d7fa84:
`FAIL negative control: a gate that prints while idle is alive`, with every
other case in the script passing; 766/767, nothing else red. Run solo, `just
check gate-host-lock` passed 3 of 3 on the same tree.

This is issue 2872's class (a timing control whose margin load can erase), in a
case 2872's fix did not reach.

## Fix

The class is "a case that must OUTLIVE the hang limit". Both members now run on
one shared margin, `alive_hang_s=5`, with the run held at three times it
(`alive_run_s=15`), so each still spans several detection windows and passing
is not vacuous. A gap would now have to exceed 5 s, ten times the tick
interval, to read as hung.

The kill-direction cases (a silent idle gate is killed, the wall limit kills a
slow verdict, housekeeping limits) keep their 2 s limits: load can only delay a
kill, and their bounds are lower bounds load widens.

## Evidence

- Mutation, detector ignores output growth: the printing control goes red, and
  only it.
- Mutation, detector ignores CPU growth: the CPU-burning control goes red, and
  only it.
- A deschedule, made deterministic: the ticking gate's process tree is
  SIGSTOPped for 3 s mid-run. With the old 2 s limit the detector kills it,
  `gate hung: no progress`, rc 1; with the new 5 s limit it runs to the end,
  rc 0. That stall is what the loaded gate produced by chance.
- Ten rounds of the fixed script under a running `just check fast` and
  `just ci gate` (host load 3.4-10.6 on 4 cores): all green. The old script,
  run alternately in the same rounds, was green too, so load alone at that
  level does not reproduce it reliably; the stall above does.
- Cost: about 20 s more per run.

## Resolution

Fixed in the test, as above.
