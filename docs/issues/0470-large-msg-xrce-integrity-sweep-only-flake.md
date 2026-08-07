---
id: 470
title: "`large_msg::test_xrce_e2e_integrity` fails only inside the full sweep — every received sample reports `valid=false`"
status: open
type: bug
area: rmw
related: [issue-0422, issue-0467]
---

## Symptom

`nros-tests::large_msg test_xrce_e2e_integrity` fails inside a full
`just test-all` run, with the XRCE listener reporting every sample as corrupt:

```
Received: seq=22 size=64  valid=false
Received: seq=23 size=64  valid=false
Received: seq=24 size=64  valid=false
Received: seq=2  size=256 valid=false
Received: seq=25 size=64  valid=false
...
```

Note the interleaving: `seq=2 size=256` arrives in the middle of the `size=64`
run. The test drives several payload sizes, so samples from different size
classes are in flight together and `valid=false` is reported for all of them,
not for one size.

## It is sweep-only

| context | result |
| --- | --- |
| full `test-all` (1259–1270 tests, parallel) | FAILS, repeatedly |
| solo (`--test large_msg test_xrce_e2e_integrity`) | **passes**, 5.0 s |
| solo, second run | **passes**, 5.1 s |

Two full-sweep runs failed it and every solo run passed. Unlike #0467 — which
also surfaced in a sweep but then failed 3/3 SOLO on an idle box — this one has
never been reproduced outside the sweep.

## Why that matters, and why it is still worth an issue

CLAUDE.md's rule is to retest a sweep red solo before filing, precisely because
QEMU/e2e lanes flake under load. This test obeys that rule, so the honest
reading is "load-sensitive", not "broken".

But `valid=false` is a payload-INTEGRITY verdict, not a timeout or a missing
message. The listener received the samples and judged their contents wrong. A
genuinely load-induced failure normally shows up as absence (no delivery, a
timed-out wait), not as delivered-but-corrupt. So one of these is true and they
need different fixes:

1. **The check is load-sensitive, not the data.** e.g. the validator compares
   against an expected pattern derived from a shared/racy source, or the
   fixture's XRCE agent is shared with another concurrent test and sequence
   state is crossed. Then the TEST is at fault.
2. **The data really is corrupt under concurrency**, and solo runs simply never
   apply the pressure that exposes it — in which case this is a real
   large-message/fragmentation defect in the XRCE path that the current
   isolation hides.

Nothing gathered so far distinguishes them.

## First step

Run the sweep with this test's XRCE agent isolated (its own port/domain via the
`nros_tests::alloc` allocator) and see whether it still fails. That separates
"shares an agent with a concurrent XRCE test" from "corrupt under load" in one
run, without needing to reproduce the whole sweep by hand.

If it survives isolation, capture one failing sample's bytes alongside the
expected pattern — `valid=false` currently reports the verdict but not the
evidence, which is the same gap that made #0448 take three sessions.

## Notes

#0422 recorded this test as "now PASSES" after its earlier triage, so this is
either a regression or a flake that its runs did not happen to hit. Observed
2026-08-06/07 across several full runs while verifying the #0447/#0448/#0458
fixes; it is NOT attributable to those (they touch the action client's CDR
framing, the linux board's tier registration, and an `nros-cpp` handle tag —
none on the large-message pub/sub path).
