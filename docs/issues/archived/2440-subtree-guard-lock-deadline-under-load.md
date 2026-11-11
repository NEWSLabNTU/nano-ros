---
id: 2440
title: "`check subtree-guard` gave its launcher a fixed 15 s to write its lock, and went red on a loaded host"
status: resolved
type: bug
severity: low
area: [gates, testing]
related: [0762, 2221]
found: 2026-11-11
resolved: 2026-11-11
resolved_in: "start_launcher waits on the launcher's own edge: lock written, or launcher exited"
---

> **Resolved (2026-11-11).** The start waits in
> `packages/testing/nros-tests/tests/subtree_guard.sh` now wait on the
> started process's own edge: ready, or exited (a failure, with its log).
> A 300 s outer bound catches only a hang.

## Measured

A local `just ci gate` (2026-11-11, newslab-118, docs-only change) failed at
step 4, `check::pristine-checkout`:

```
FAIL [  20.031s] (514/518) nros-gates::gates fast::check::subtree-guard
  ok: SIGTERM to the launcher killed the whole subtree
  FAIL: launcher never wrote its lock file (/tmp/tmp.gL4G4rUpet/guards/checkout-2200659235/guardtest.pgid)
```

The host was loaded: the pristine-clone pass of the fast tier ran beside
another worktree's `just check fast`. The same gate passed in that
worktree's own `check fast` minutes earlier.

## Why

`start_launcher` waited with `wait_until 15 test -s "$(guard_lock)"`, a fixed
deadline on a launcher that was still starting. It is a deadline sized for
an idle host, which CLAUDE.md's rule calls a start race: a flaky start is
fixed at its readiness edge, never by a larger number or a retry
(issue 2221).

## Fix

One helper, `wait_for_start <pid> <what> <log> <predicate…>`, waits for a
process the test started to reach its readiness edge. It returns when the
predicate holds. It fails at once, with the process's log, when the process
exits without it. It fails after 300 s only when the process is still running
and never got there, a hang that no load explains. It polls every 0.2 s. It
replaces all three start waits in the file:

- `start_launcher`: the lock file (the reported failure);
- `start_launcher`: the payload tree's depth >= 3. That wait ended in
  `|| true`, but path 1 asserts the depth right after it, so it was a 15 s
  assertion too;
- the `sibling-a` pool's guard lock (`pool_lock_held`, 15 s).

Left as they were, on purpose: the four `wait_until` calls after a signal
(the SIGTERM drain, 20 s; the lock removal, 10 s; the SIGKILLed launcher's
death, 10 s; the orphan reap, 30 s). They wait for processes to DIE after a
kill, which is not a start, and a longer death there is itself a finding.

Sweep: `grep -n "wait_until\|wait_for_start" packages/testing/nros-tests/tests/subtree_guard.sh`.
