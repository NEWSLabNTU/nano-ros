---
id: 923
title: "`just test` spawns peers and never sweeps them — only `test-all` runs
  issue 0659's reaper"
status: resolved
type: bug
area: testing
related: [issue-0659, issue-0196]
---

## Measured

67 orphaned peer processes on the `ros2` distrobox host, 2026-08-30:

```
33  talker
18  zenohd
16  add_two_ints_server
```

oldest 3 h 13 m, plus one `demo_nodes_cpp talker` at 11 h 34 m, load average
2.02 from processes nothing was waiting on. Issue 0659's symptom exactly: it
measured 59 orphaned `add_two_ints_server`, oldest 9.4 days, holding domain-5
discovery ports until an unrelated cyclone test failed with `failed to bind to
ANY:8650: address in use`.

## The gap

0659's reaper is real and IS wired — `justfile:1872`, at the head of `test-all`,
before nextest starts:

```
cargo run -q -p nros-tests --bin nros-peer-sweep 2>/dev/null || true
```

It is the ONLY invocation in the tree. `just test` (`justfile:1220`) runs the
same nextest suites, spawns the same peers, and does not sweep. So a SIGKILLed
`just test` — a nextest timeout, an interrupted lane, a killed harness, none of
which run `Drop` — leaves its recorded groups in the ledger until somebody
happens to run `test-all`.

`PR_SET_PDEATHSIG` cannot cover this: it delivers `SIGKILL` to `bash` only, and
`timeout`/`ros2`/the node reparent to init. The design says so and names the
sweep as the backstop; the backstop just is not on every lane that needs it.

## Correction to this issue's first draft

This was first filed as *"reachable from no recipe, no Justfile line and no
workflow"*, on the strength of:

```
$ grep -rn 'peer-sweep' just/ Justfile .github/workflows/
(no matches)
```

That grep is wrong and the conclusion drawn from it was wrong. The root recipe
file is lowercase `justfile`; `Justfile` does not exist, so the pathspec matched
nothing and the empty result read as "no callers". `ugrep` warns
`No such file or directory` on stderr, which was not read. The wiring has been
there all along — the defect is narrower and is the one above.

Recorded rather than quietly rewritten: an absence proven by a search is only as
good as the search, and this is the second time in one session that a
case- or path-wrong pattern produced a confident wrong answer.

## Fix

1. Sweep at the head of `just test` as well, not only `test-all`.
2. Gate it, so a lane that spawns peers cannot be added without one — the
   sweep's own unit tests call `sweep_in` directly and can never see whether a
   lane calls it.

## Related, and NOT this bug

The 33 `talker` and 18 `zenohd` above did not come from the harness at all: a
hand-written acceptance script spawned them with `setsid nohup`, which detaches
from the process group the ledger records, so they never entered this mechanism.
A hand-run repro that spawns peers leaks by construction unless it goes through
the same helper.


## Resolution (2026-08-30)

Fixed on three levels, because the reported gap was the shallowest of them.

**1. The lane.** `just test` now sweeps before nextest, exactly as `test-all`
does. That closes the measured hole: a SIGKILLed `just test` no longer waits for
somebody to run the other lane.

**2. The gate.** `check-peer-sweep-lanes`, on the fast line. Mutation-tested —
removing the call from `just test` fails it, restoring it passes — because the
reaper's own unit tests call `sweep_in` directly and can never see whether a
LANE calls it. It carries its own selftest on the normal path, per
`check-gate-selftests`.

**3. The kill switch, which is the part worth keeping.** Sweeping at lane start
means orphans live from the moment a run dies until the next run begins. The
reason that was accepted is real — `PR_SET_PDEATHSIG` delivers SIGKILL, SIGKILL
cannot be handled, so `bash` cannot pass the news to `timeout`/`ros2`/the node.

But SIGKILL is a CHOICE, not a constraint. `set_orphan_group_suicide` asks for
**SIGTERM**, which `bash` can trap, and `group_suicide_wrapper` puts the trap on
the command text: parent dies -> kernel SIGTERMs `bash` -> the trap does
`kill -TERM 0` (its whole process group, which `setpgid(0, 0)` just made
exclusively ours) -> the descendants die at that moment. A `getppid() == 1`
guard in `pre_exec` covers the race where the parent died between fork and
`prctl`, when the signal would never come.

The trade is deliberate and bounded: a peer that IGNORES SIGTERM survives where
SIGKILL would have reaped it. So the wrapper follows with `kill -KILL 0` after a
grace, and the ledger sweep stays as the backstop for the case where `bash`
itself is SIGKILLed.

## Two errors made while fixing this, recorded because both were self-inflicted

**The first draft of this issue was wrong** — see the correction above. An
absence proven by `grep` is only as good as the pathspec, and `Justfile` does
not exist in this tree.

**The gate's first selftest passed vacuously.** Its awk had broken quoting, so
the extraction returned an empty string and the "did the body leak into the next
recipe?" check found no match in nothing and reported success — the exact
failure mode the control exists to catch, inside the control. Fixed by asserting
non-empty FIRST, and by factoring the extraction into one function the selftest
and the real check share, so the control exercises the logic that ships rather
than a copy of it.
