---
id: 1762
title: "`unique_ros_domain_id` hands two concurrent tests the SAME domain once its busy-stepping kicks in"
status: resolved
type: bug
area: testing
severity: medium
found: 2026-10-09
related: [issue-0707, issue-1333, issue-1732, phase-480]
---

## What happens (measured, 2026-10-09)

While issue 1732's regression test was being written, two cases in one
`rust_multi_node_per_node_graph` run ran in parallel. Each logged the domain it
got from `nros_tests::unique_ros_domain_id()` and the Agent it started:

    Clean   domain=9 agent=127.0.0.1:38007
    Sigterm domain=9 agent=127.0.0.1:57077

Both got domain 9, from two different nextest slots in the same run. The
SIGTERM case then read the clean case's `/talker` and `/listener` for about
10 s after its own image had closed its session. That is the clean image's
remaining run time, not a leftover of its own. Run alone, the same case
passed in 2.5 s, three times out of three.

## Why (read from the source)

`domain_in_slot(slot, seq)` gives each slot a 4-domain block. When the first
candidate is busy, `domain_avoiding_busy` steps to `domain_in_slot(slot + step,
seq)`: the NEXT SLOT'S block. If domains 1 and 5 are busy (another
worktree's nextest run uses the same slot numbers on a shared host), slot 0
steps to 9 (step 2) and slot 1 steps to 9 (step 1). That path gives
the measured pair. The busy state of 1 and 5 was not captured; it is inferred
from the arithmetic. Concurrent callers converge
on the same block instead of spreading out. The busy probe cannot break the
tie, because neither caller has bound the domain yet when the other probes it.

## Why it matters

Any equality or absence assertion over a shared-name image is exposed to it:
`ros2 node list`, `topic list`, and "the image has left". A test that asserts
a SUPERSET passes by accident. Issue 1732's test defends itself, so it is not
blocked: it is one sequential case, and it re-draws a domain until `node list`
is empty. That is a workaround at one call site, not a fix.

## What a fix needs

- Step within the caller's own block (`seq + k`) before leaving it, so a busy
  first candidate does not land on another slot's first choice.
- Or claim the domain (bind its SPDP port, or a lock file) for the caller's
  lifetime, so the second caller's probe sees it as busy.
- A test that calls the function from two threads with distinct slots and a
  probe that reports the first candidates busy, and asserts distinct answers.

## Not measured

How often this happens in CI, where only one nextest run is on the host.

## Resolution

Fixed 2026-10-10 on `fix/1762-domain-claim`. Both halves of "What a fix
needs" were done, and the fix covers the CLASS: all three assigners.

**Rust (`nros_tests::unique_ros_domain_id`).**
- *Order.* `domain_avoiding_busy` now tries the caller's own block first
  (`seq`, `seq+1`, …) and only then the next slots' blocks, each from `seq`.
  With nothing busy the answer is bit-identical to before
  (`a_free_domain_is_the_same_answer_as_before` still passes).
- *Claim.* A candidate is returned only once `claim_domain(d)` succeeds. That
  is an exclusive, non-blocking `flock` on
  `$TMPDIR/nros-test-domain-claims/<d>.lock`, held for the life of the
  process. The kernel drops it on any exit, so no stale claim needs cleaning
  up. It is shared across the host, so another slot, another worktree's run,
  and the shell and C++ assigners all see it. `flock` locks belong to the
  open file description, so one process that asks twice also gets two
  domains. When a claim cannot be made (no writable temp dir, or not Unix),
  the function answers "claimed", the same degradation rule the probes use.
- Why not `port_lease`'s O_EXCL+pid files: the claim has to be taken from
  shell and C++ as well. `flock(1)` and `flock(2)` provide it directly, and a
  kernel-held lock needs no stale-owner reclaim.

**Shell (`ros2_e2e_common.sh`) and C++ (`nros_test_domain.h`).** Both had the
same convergence, one step over: they stepped `+1`, onto a neighbour's first
choice. Both now claim through the same files.
- `nros_unique_ros_domain_id` now sets and exports `ROS_DOMAIN_ID` in the
  current shell and prints nothing. Inside `$(...)`, a subshell would release
  the claim when it exits, and the empty output makes that misuse fail loudly.
  Both callers were updated.
- `nros_test_domain()` caches its first answer, because the header's contract
  is that every session in one process resolves to the same value.

**Gate.** `check-test-domain-assignment` now also requires every assigner to
name the shared claim directory. It was mutation-tested: renaming the
directory in the header fails the gate.

**Measured:**
- New unit tests: `two_concurrent_callers_whose_first_choices_are_busy_get_distinct_domains`
  (two threads, slots 0 and 1, domains 1 and 5 busy, a shared fake claim
  registry), `a_busy_first_choice_steps_within_the_callers_own_block`,
  `a_claimed_domain_is_not_handed_out_again` and
  `the_lock_file_claim_refuses_a_second_holder` (the real flock).
  **Negative control:** with the old function body restored, the three
  algorithm tests fail with `(9, 9)`, `SAME domain 9` and `converged on 9`,
  which is the issue's measured pair exactly.
- Shell: two concurrent shells with the same slot got domains 1 and 2. With
  the old function both would have taken 1.
- C++: a probe binary, run with every other test domain flock'd by a shell
  through the shell's claim files, answered 77, and 77 again on its second
  call.

**Issue 1732's `quiet_domain`: kept, with the reason rewritten.** The claim
removes the convergence it was written for. What it still guards is a process
that never took a claim: an image orphaned past its test (a spawned child
does not inherit the CLOEXEC lock), or a `ros2` node started by hand. Neither
binds anything under zenoh, so the busy probe misses them too, and that
test's verdict is an absence.

**Sweep:** `git grep -n "unique_ros_domain_id\|nros_unique_ros_domain_id\|nros_test_domain("`.
This covers all 3 assigners and every caller. Callers needed no change: each
already used one domain per call.

**Not measured:** a full fixture-backed e2e run of the Rust callers. The
native fixtures were not built on this branch, and
`native_example_pubsub_e2e` refused 9 of 10 cases as not prebuilt. The C++
and shell assigners were run end to end: `just check rmw-cyclonedds` passed
34/34, including `ros2_pubsub_e2e` and `ros2_srv_e2e`, which go through the
new in-shell `nros_unique_ros_domain_id`. How often the convergence fired in
CI is still unknown.
