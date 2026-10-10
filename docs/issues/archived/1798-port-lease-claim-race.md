---
id: 1798
title: "`port_lease` could hand one port to two fixtures: a lease was created EMPTY and its pid written afterwards, so a concurrent claimer read it as a dead owner and stole it"
status: resolved
type: bug
severity: medium
area: [testing]
related: [0470]
found: 2026-10-11
---

## Measured

During #1838's `just ci gate`, `port_lease::tests::concurrent_leases_are_distinct`
failed once inside `test-lane-contracts`. It then passed both run alone and
in a full re-run. That test asserts the one property issue 0470 built the
module for: two leases never name the same port.

Reproduced on main (`94749a292d`) with 16 parallel loops of the test:
**1 failure in 640 runs**. A further 960 runs did not fail. The race is rare
at test scale because it needs the kernel to suggest the same ephemeral port
to two claimers at almost the same moment.

## Cause

The reservation was a lock file created with `O_EXCL` that held the owner's
pid. A lease whose pid was dead got reclaimed. That design had two races,
and both hand one port to two fixtures:

1. **Create, then write.** `claim()` created the file with `create_new` and
   wrote the pid in a second step. A concurrent claimer that hit the same
   port could read the file while it was still EMPTY, and `owner_is_gone()`
   treated "no parseable pid" as "dead owner". That claimer then deleted the
   live lease and took it.
   - A deterministic test against the old code fails:
     `OLD CODE: an empty (in-progress) lease was stolen`.
2. **Remove, then create.** Two reclaimers could both see a dead owner and
   interleave `remove_file` and `create_new`, so the slower one deleted the
   faster one's fresh lease.

This is the cross-talk class issue 0470 exists to prevent: two "unique" XRCE
agents or routers end up on one port, and the symptom is delivered-but-wrong
data in an unrelated test.

## Fixed

`packages/testing/nros-tests/src/port_lease.rs`: on unix the reservation is
now an exclusive non-blocking `flock`, held by the lease's open descriptor.
It is no longer a pid in the file.

- **No empty-file window.** The file can exist while nobody holds it, but that
  only means the port is free. Holding the lock is what reserves it.
- **No liveness check.** The kernel drops a dead or SIGKILLed process's
  `flock`, so a leaked file reserves nothing. `owner_is_gone` / `pid_is_live`
  (`kill(pid, 0)`) are deleted.
- **The drop-time unlink race is closed.** Drop unlinks the path while it still
  holds the lock. A racer whose descriptor was opened before that unlink can
  later win the lock on the orphaned inode. `lock_if_current()` therefore
  accepts a lock only if the path still names the locked inode
  (`dev` + `ino`).
- **Non-unix** keeps exclusive creation and never reclaims. A leaked file costs
  a retry on another port, never a shared one.
- The pid is still written into the file, as a diagnostic only.

## Tests

| test | proves |
| --- | --- |
| `concurrent_leases_are_distinct` | 200 leases over 8 threads, all distinct (kept) |
| `a_held_lease_with_no_pid_yet_is_not_stolen` | the race above, deterministically: an empty file held by a live claimer is refused |
| `a_dead_owners_lease_is_reclaimed` | a leaked, unheld file (any content) is free; once held, a second claim is refused |
| `dropping_a_lease_releases_it` | drop removes the file and the port can be leased again |
| `a_lock_won_on_an_unlinked_file_is_not_ownership` | a descriptor opened before the owner's unlink can win the lock on the orphan, and that must not count as owning the port |

Mutations (the failing tests in each case; clean tree 5/5):

- Drop the inode check: `a_lock_won_on_an_unlinked_file_is_not_ownership`
  fails.
- Skip the `flock`: `a_held_lease_with_no_pid_yet_is_not_stolen` and
  `a_dead_owners_lease_is_reclaimed` fail.
- Run the old implementation against the deterministic test: it fails
  (the in-progress lease is stolen).

Stress after the fix: 16 parallel loops gave **0 failures in 960 runs**. The
old code gave 1 in 640, so that number is supporting evidence. The
deterministic test is the proof.
