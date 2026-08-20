---
id: 707
title: "Every solo or filtered test run takes ROS domain 1, so an orphan from the last run joins the next one"
status: resolved
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

## Fixed 2026-08-20 — direction 3, because it keeps what direction 2 spends

Took "probe and step". The issue left the choice open and framed it as
reproducibility versus isolation; direction 3 is the one that does not actually
trade them, and that is why it wins rather than because it is the most thorough.

* **Determinism is kept where it was worth having.** With nothing squatting the
  first candidate is free and the answer is BIT-IDENTICAL to the old partition —
  asserted over 30 slots x 6 seq, not argued. A failing run is still re-creatable
  by hand.
* **It moves only when reuse would be wrong.** Salting the base (direction 2)
  pays the reproducibility cost on every run, including the overwhelming
  majority where domain 1 is perfectly free.
* **Orphan cleanup (direction 1) remains worth doing** and is issue 0659's; this
  does not close it. What it removes is the systematic half — that the FIRST
  party is always the same.

### The probe

RTPS derives its ports from the domain, so "is somebody on domain d" is
answerable locally without joining the bus: SPDP's multicast port is
`7400 + 250*d`, and every participant on that domain binds it. The probe reads
`/proc/net/udp{,6}` rather than attempting a bind, because `SO_REUSEADDR` on a
multicast socket means a successful bind proves nothing.

Local by design. The orphan this dodges is a process the last run left on THIS
host; a peek that needed real discovery would have to create the participant it
is trying to place. Non-Linux or unreadable `/proc` answers "not busy", so the
assignment degrades to exactly the old behaviour — a probe that cannot see must
not invent.

It only helps DDS, and that is the right scope: the cross-talk this issue
recorded was a Fast-DDS reader receiving another type's payload. zenoh reaches
its peers through a router port, which `port_lease` already makes exclusive
(issue 0470).

### Verified

* `a_free_domain_is_the_same_answer_as_before` — the determinism property, over
  30 slots x 6 seq.
* `an_occupied_domain_is_stepped_over` — asserts the hazard's precondition
  (slot 0, seq 0 => domain 1) still holds, then that an occupied domain 1 is
  left behind.
* `every_domain_busy_still_returns_one` — giving up yields a domain rather than
  hanging; a caller with none has nowhere to go.
* `the_probe_sees_a_real_bound_discovery_port` — both directions against the
  kernel's own table with a socket actually bound, and MUTATION-CHECKED: flip
  the probe's `true` to `false` and this test fails. A probe that stopped
  probing would otherwise look exactly like a quiet host.

151 `nros-tests` lib tests pass.

### Not claimed

That this fixes the sweep failure the issue reported. That failure was never
attributed to a domain collision — the issue is explicit that the bus was not
established to be domain 1 — and its readiness-wait defect was fixed separately.
This closes the systematic hazard, not that observation.

