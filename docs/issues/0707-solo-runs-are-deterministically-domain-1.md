---
id: 707
title: "Every solo or filtered test run takes ROS domain 1, so an orphan from the last run joins the next one"
status: open
type: bug
severity: medium
area: testing
related: [issue-0672, issue-0703, issue-0659]
---

## The trigger issue 0672 named

Issue 0672 closed with a hazard it deliberately did not fix:

> `unique_ros_domain_id()` is not unique for a filtered/solo run: with
> `NEXTEST_TEST_GLOBAL_SLOT=0` and `seq=0` it returns **domain 1** every time.
> Any orphaned DDS participant left on domain 1 by an earlier run therefore
> lands in the next one. […] Worth its own issue if a domain-1 collision is ever
> observed directly — it was not observed here, only noted as reachable.

Filed on that instruction. Still true after #703's ceiling change and after the
partition that followed it: slot 0, seq 0 → `0*4 + 0 + 1` = **domain 1**.

## What was observed, and how far it goes

The 2026-08-20 tier-1 sweep's one real failure was
`nros-tests::xrce_ros2_interop test_xrce_service_ros2_client`, 3/3 retries. Its
third retry carried:

```
[RTPS_READER_HISTORY Error] Change payload size of '28' bytes is larger than the
history payload size of '15' bytes and cannot be resized.
   -> Function can_change_be_added_nts
```

A Fast-DDS reader sized for one type receiving a change of another size is
cross-talk: a participant on that bus that the test did not put there. That is
the shape 0672 predicted.

**What is NOT established:** that the bus was domain 1. The message carries no
domain, the run was in-sweep (so the test had a real slot, not slot 0), and the
failure did not reproduce solo, under full CPU saturation, or with three interop
binaries running concurrently. So this records a cross-talk observation
consistent with the hazard, not a confirmed instance of it. Saying otherwise
would repeat 0672's own correction, where a confident attribution to a commit
window turned out to be wrong.

## Why determinism is the defect, not the collision

A collision needs two parties, and the second one is an orphan — which is
issue 0659's class (this host was carrying days-old `zenohd` processes when 0672
was investigated). Orphan cleanup is a separate fix and does not close this.

What makes the hazard *systematic* rather than unlucky is that the first party is
always the same: every `cargo nextest run -E 'test(…)'`, every hand-run repro,
every solo retest of a red — the workflow CLAUDE.md explicitly prescribes ("retest
a QEMU red SOLO before filing") — runs on domain 1. So the one situation in which
an engineer is *most* likely to be chasing a ghost is the situation guaranteed to
reuse the same bus as the run that left the ghost.

## Directions, none free

* **Do nothing, fix orphans instead** (0659). Cheapest; leaves the determinism, so
  the next orphan from any source reproduces this.
* **Salt the base with something per-run** — e.g. fold the process start time or a
  nextest run id into the block base. Breaks nothing that pins `ROS_DOMAIN_ID`
  (case 1 of the assigner still wins), but makes a failing run harder to
  re-create by hand, which is the property the current scheme was chosen for.
* **Probe and step**: try the computed domain, and if SPDP shows a participant
  that is not ours, take the next block. Correct, and the most code; needs a
  discovery peek before the test's own participant exists.

Not decided here. The tradeoff is genuinely between reproducibility and
isolation, and the current scheme picked reproducibility without recording that
it had.

## Note on the sweep failure itself

Not attributed to this issue. `test_xrce_service_ros2_client` discarded its
server-readiness wait (`let _ = wait_for_output_pattern(…, 5s)`), so no failure of
it could distinguish "server never came up" from "server replied wrong" — which
is 0672's actual root cause, fixed there in the reverse-direction test and not
carried across the file. That was fixed separately; the next occurrence will say
which failure it is.
