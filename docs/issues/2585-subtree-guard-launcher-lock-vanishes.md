---
id: 2585
title: "subtree-guard's test reads its lock twice; the lock can vanish between the reads, and the launcher log that would say why is deleted"
status: open
type: flaky-test
area: [testing, build]
severity: low
found: 2026-11-15
related: [issue-2221, issue-2440]
---

## What happened

On 2026-11-15, at 21:22Z, `just ci gate` step 4 (`check::pristine-checkout`)
went red on one gate only:

    FAIL [   3.570s] (447/539) nros-gates::gates fast::check::subtree-guard
      FAIL: could not signal launcher

The pid in that message is EMPTY. The same branch passed `just check fast` in
its normal worktree just before. Run solo afterwards, `just check
subtree-guard` passed 3 of 3. The branch does not touch the guard or its test.

## What the message tells us

`packages/testing/nros-tests/tests/subtree_guard.sh` path 1 reads the lock
file three times:
1. `start_launcher` waits until the lock is non-empty and the payload tree is
   at depth 3 or more.
2. `payload_pgid` reads the lock: non-empty, and the depth check passed.
3. `launcher_pid` reads it again and gets nothing.

So the lock was removed or emptied between reads 2 and 3. This is not a
collision with another run: `NROS_GUARD_LOCK_DIR` is the test's own
`$work/guards`. The guard removes the lock when the group drains, so the likely
reading is that the launcher exited early. This run was under pristine-checkout
load, with 539 gates sharing the host.

That stays a reading, not a diagnosis. `$work/launcher.log` would say why the
launcher exited, but it is deleted with `$work` on exit.

## Fix direction

- Read the lock ONCE (pid, pgid and tree from one `read`), so the three values
  describe one state.
- When a lock that was there is gone, fail naming that, with
  `launcher.log`'s content, instead of an empty pid.
- Then find the start race the log shows (CLAUDE.md: no retries; a flaky start
  is a race to fix).

## Second sighting, a different message (2026-11-19)

At 08:09Z, `just ci gate` step 5 (`check::pristine-checkout`) on the issue-2624
branch (which does not touch the guard) went red on this gate alone:

    subtree_guard.sh: line 137: <pid> Killed  "$work/launcher.sh" > "$work/launcher.log" 2>&1
    FAIL: a second pool with the SAME label started while the first ran — the
    guard no longer refuses a concurrent build of one artifact set

Its first four assertions passed in the same run. The host had 1,243 runnable
threads at the lane's busiest sample, 699 of them another tenant's
`component_node` processes; load average 143 a minute later. Run solo
right after, `just check subtree-guard` passed 3 of 3.

So, under heavy load, the guard's concurrent-build refusal path also fails. As in
the first sighting, the launcher appears to leave before the assertion reads
its state: `Killed` is the test's own SIGKILL path at line 137. The
fix direction above (read the lock once, keep `launcher.log` on failure) covers
this case too.
