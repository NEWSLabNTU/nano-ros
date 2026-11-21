---
id: 2814
title: "`just ci gate` run under a holder of the host gate lock waits on its own parent until the wall limit, then reports a timeout with nothing run"
status: resolved
type: bug
severity: medium
area: [ci, tooling]
related: [2602, 2733]
found: 2026-11-22
resolved: 2026-11-22
resolved_in: "fix(ci, issue 2814): a gate under its own ancestor's gate lock refuses at once"
---

## What happens

`just ci gate` takes the host gate lock itself
(`just/ci.just`: `bash scripts/ci/gate-host-lock.sh --housekeeping … just ci::_gate-inner`).
If the caller already holds that lock, for example by running
`bash scripts/ci/gate-host-lock.sh just ci gate` the way a heavy build is
wrapped, then the inner `gate-host-lock.sh` waits for the outer one. The outer
one is its own ancestor and cannot let go until the inner one finishes.

Measured on newslab-175 (2026-11-21, 20 cores, wall limit 9000 s):

```
ci gate: holding this host's gate lock: wall limit 9000 s (150 min, 20 cores), …
ci gate: this host's gate lock is held: pid=1717835 worktree=…/p-orch-migrate branch=fix/orch-sched-fixtures-entry-image started=2026-11-21T15:09:06Z (alive)
  waiting for it (issue 2602: one gate per host): …
ci gate: still waiting for the gate lock: pid=1717835 … (alive)      # ×30, one per 5 min
ci gate: gate timed out after 9000 s; its process group was killed, and the host's gate lock is free (issue 2602).
```

The exit code was 1. None of the 11 steps ran. pid 1717835 was the outer
wrapper, which is the waiter's own ancestor, in the same worktree and on the same branch.

## Why it costs the whole wall limit

- The waiter's cap (wall + housekeeping + 30 min) is longer than the holder's
  wall limit, so the waiter never gives up first. The outer holder's wall limit
  is what ends the deadlock.
- The hang detector does not fire. It counts new output as progress, and the
  waiter prints a "still waiting" line every 5 min.
- The verdict reads "gate timed out", which is the message for a gate that ran
  too long. Here nothing ran. A reader takes it for a slow or hung gate, not a
  wrong invocation.

## Fix direction

When the holder recorded in the lock file is an ancestor of the waiter,
`gate-host-lock.sh` should act at once, not wait. Two options:

1. Refuse immediately. Name the ancestor holder and say that `just ci gate`
   takes the lock itself, so it must not be wrapped. Exit with the existing
   NOT RUN status, which is distinct from any gate verdict.
2. Treat the ancestor holder as "already held" and run the command under the
   existing lock (the `--held` path) without taking it again.

Option 1 keeps "no path runs a gate without its own limits". Under option 2
the inner gate would run under the outer gate's limits.

Acceptance: `bash scripts/ci/gate-host-lock.sh bash scripts/ci/gate-host-lock.sh true`
finishes in seconds and does not reach the wall limit. Under option 1 it
fails, naming the ancestor. The check goes in the lock script's own tests,
using the `_NROS_GATE_LOCK_TEST_*` limits.

## Resolved, 2026-11-22

The preferred direction: refuse, naming the holder, and keep every limit.

Before it waits, `scripts/ci/gate-host-lock.sh` reads the holder's `pid=` line
from the lock file. It then walks its own parent chain through `/proc/<pid>/stat`
(field 4, read after the last `)`, so a command name with spaces cannot shift
it). If the holder is an ancestor, the script exits 1 at once with:

    ci gate: NOT RUN — this host's gate lock is held by pid <p>, an ANCESTOR of
    this process: a gate inside a gate's lock would wait on itself until the
    waiter cap (<cap> s). Run the inner command without the lock wrapper; the
    outer holder already serialises the host.

An unrelated holder is still waited for, exactly as before.

Acceptance: `gate-host-lock.sh gate-host-lock.sh true` with
`_NROS_GATE_LOCK_TEST_WAIT_S=120` refuses in 0.04 s, naming the ancestor.

`tests/gate-host-lock-tests.sh` case 16 asserts it, bounded at half the
120 s cap. Mutation-tested: with the check disabled, the case goes red after
121 s, which is this bug's wait. Case 2 is the negative control: an unrelated
holder is still waited for.

