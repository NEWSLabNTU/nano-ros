---
id: 1762
title: "`unique_ros_domain_id` hands two concurrent tests the SAME domain once its busy-stepping kicks in"
status: open
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
