---
id: 2585
title: "subtree-guard's test reads its lock twice; the lock can vanish between the reads, and the launcher log that would say why is deleted"
status: resolved
resolved_in: "the change that archives this issue"
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

## Cause: two races, both in the test

**Sighting 1, the lock vanishes.** The test's cleanup ran
`pkill -f nros-guard-test-payload`, mid-test (after path 3) and in its EXIT
trap. That name was the same for every run of the test on the host. So
another run elsewhere, such as another worktree's `check fast` or a pre-push
hook, killed THIS run's payload leader. Only the leader carries the name.
The launcher's `wait` then returned, so it removed its lock and exited,
while the leader's two `sleep` children kept the group at depth 3.

Measured directly: a launcher holding its lock, then one foreign
`pkill -f <name>`. Afterwards the lock is gone, the launcher is gone, and the
group still has 3 members. That is sighting 1's state: the depth check
passed, the pgid read succeeded, and the pid read found nothing.

**Sighting 2, the same-label pool starts.** Path 5 held `sibling-a`'s guard
for a fixed 8 s. Under a load average of 143, starting and finishing
`sibling-b` took longer than that, so `sibling-a` had already released and
the same-label pool found no holder. Measured: with `sibling-b` delayed by
10 s, the old test fails with sighting 2's exact message. The `Killed` line
beside it is bash's notice for path 3's own SIGKILL, and is not the failure.

## Fix

- The payload carries this run's own name, `nros-subtree-guard-payload-<scratch
  dir>`. It shares no prefix with the old name, so an old run elsewhere
  cannot match it either. Cleanup kills the recorded group as well as the
  name.
- The lock is read once (`read_lock`). A lock that is gone or empty fails as
  "the launcher left early". Every failure prints the tail of each scratch
  log, which used to be deleted with the scratch dir.
- Path 5's stub holds its guard until the test releases it, never for a
  duration. It also lets go if the scratch dir disappears, so a failed test
  leaves no pool waiting.
- The reaping launcher of path 3 is stopped once it is ready. Before, every
  run leaked one payload group for 240 s.

Negative controls, each a copy of the test with one line injected:

| control | old test | new test |
| --- | --- | --- |
| foreign `pkill -f nros-guard-test-payload` after path 1 starts | FAIL `no payload pgid recorded` | pass |
| this run's own leader killed after path 1 starts | n/a | FAIL "the guard's lock ... is gone or empty ... the launcher left early" |
| `sibling-b` delayed 10 s | FAIL "a second pool with the SAME label started" | pass |
