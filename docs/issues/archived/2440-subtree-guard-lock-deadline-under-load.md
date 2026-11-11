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

> **Resolved (2026-11-11).** `start_launcher` in
> `packages/testing/nros-tests/tests/subtree_guard.sh` now waits until the
> launcher writes its lock or exits. If it exits without one, the test fails
> with the launcher's log. A 300 s outer bound catches only a launcher that
> hangs without doing either.

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

The launcher has exactly two outcomes that matter: it writes the lock, or it
exits. The wait now polls for both (every 0.2 s). It fails at once, with
`launcher.log`, when the launcher exits without the lock. It fails after 300 s
only when the launcher is still running and has written nothing, a hang
that no load explains. The later `wait_until 15` for the tree's depth is
unchanged: it ends in `|| true` and asserts nothing.

Sweep: `grep -n "wait_until [0-9]" packages/testing/nros-tests/tests/subtree_guard.sh`.
The only other one is that non-asserting depth wait.
