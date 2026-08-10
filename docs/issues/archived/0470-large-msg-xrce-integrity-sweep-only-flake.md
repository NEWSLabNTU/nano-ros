---
id: 470
title: "`large_msg::test_xrce_e2e_integrity` fails only inside the full sweep — every received sample reports `valid=false`"
status: resolved
resolved_in: phase-342
type: bug
area: rmw
related: [issue-0422, issue-0467, issue-0501]
---

## Resolution (2026-08-11) — cross-talk, not corruption. Option 1, twice over.

The issue's two candidates were "the check is load-sensitive" vs "the data is
corrupt under concurrency". It is the FIRST, and the decisive evidence was in
the failure output all along:

```
Received: seq=0 size=64  valid=false      <- a NEIGHBOUR's traffic
Received: seq=0 size=512 valid=true       <- this test's own, always valid
```

This test publishes 512-byte payloads. **Every 512-byte sample was valid in
every observed failure**; only the foreign 64-byte ones were "corrupt". The
payload path was never involved — a second publisher's samples were arriving in
this listener's subscription, and the validator judged them against the wrong
expected size.

Two independent isolation leaks, both fixed:

**1. The "unique" agent port was not unique.** `allocate_ephemeral_udp_port`
bound port 0, read the number and CLOSED the socket, returning a port that
belonged to nobody until the agent bound it. Measured on this host: 2400
allocations across 12 concurrent processes produced **87 colliding ports**,
several handed out three times. Replaced with `nros_tests::port_lease` — a
cross-process lock file per port, held for the fixture's lifetime, reclaimed
when its recorded pid is gone (nextest SIGKILLs, so leaked leases are expected).
Applied to the zenoh router's identical allocator in the same change: same
defect, same file shape, one class.

**2. All four XRCE stress tests shared one topic.** `/stress_test` was
hardcoded in the stress binary. Distinct agents do not isolate this: an XRCE
agent bridges its clients onto DDS, so on one host at one domain a shared topic
name is a shared bus. Added a `STRESS_TOPIC` knob; each test now names its own.

Port isolation alone did NOT fix it — verified by fixing the ports first and
watching the failure persist. That is what pointed past the transport.

### The comment that hid it

Both allocators carried the same reasoning: *"safe for nextest where each test
runs in a separate process — a static counter would reset per process and cause
port collisions."* True premise, wrong conclusion. Separate processes are
exactly why an in-process scheme cannot work, and why the reservation has to be
on disk. `large_msg`'s `XRCE_LARGE_MSG_LOCK` is a process-local `static Mutex`
with the same flaw — it serialises nothing between nextest's test processes, so
the four XRCE tests were always concurrent. It is left in place (harmless) but
it is not what makes them safe; the topic split is.

### Both old guards were untestable by construction

Each allocator had a unit test asserting two SEQUENTIAL allocations differ.
That holds for the racy allocator too — the kernel only re-hands a port once the
first is released. Neither test could have failed on the defect it existed to
guard. Replaced with "distinct while HELD".

### Verified

`large_msg` 11/11 XRCE-clean ×3 consecutive runs (was: fails ~every run once its
siblings run concurrently). Tripwired: removing the topic split alone brings the
failure straight back. `nros-tests --lib` 109/109 including three new
`port_lease` tests; clippy `-D warnings` clean.

Note this was never truly "sweep-only" — it reproduces in the `large_msg`
binary alone, because the siblings that collide with it live there. The earlier
solo runs passed because they ran the ONE test.

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
