---
id: 1013
title: "`test_rtos_pubsub_e2e` SIGKILLs its talker after ~12 publishes, so the
  cell exercises twelve seconds of a free-running publisher"
status: open
type: bug
area: testing
severity: medium
found: 2026-09-03
related: [issue-0877, issue-0906, issue-1005, phase-414]
---

## What happens

`wait_for_output` is a RUN-TO-COMPLETION wait — its own doc-comment says "wait
for QEMU to produce output *and exit*" — and `rtos_e2e.rs:729` aims it at a
free-running 1 Hz publisher with a 15 s window (`:725`). When the window
expires, `qemu.rs:448` `kill_process_group`s the guest.

    listener t=0 -> stabilization_delay 20 s -> talker t=20
    -> SIGKILL t~35 -> verdict t~35

MEASURED, all three languages, every run: the talker emits exactly **12**
publishes and is then killed. The service and action shapes do not do this —
they use `collect_until` and let the long-lived server run the whole window.

## Why it matters

**It silently bounds what the cell can observe.** Anything whose period exceeds
~12 s of session life is invisible to it, and the cell reports PASS regardless.

That is not hypothetical. Issue 0906 fixed `Z_TRANSPORT_LEASE` 10 s -> 60 s
because a 10 s lease against a 30 s router keep-alive expired every session.
Measured while accepting issue 0877: **rebuilding with the OLD 10 s lease still
passes this cell 6 of 6**, because the first lapse is at ~20 s of session life
and the window closes first.

So this cell cannot regression-test 0906, and nothing else does. Issue 1005 is
the other half — the staleness probe cannot see that constant change either, so
it is unprotected in both directions.

## Direction

Not settled; the choice is about what the cell is FOR.

1. **Use the shape the sibling cells use.** `collect_until` with a predicate on
   messages received, letting the publisher run, is what service and action do
   and why they are not subject to this.
2. **Raise the window** past the longest period the cell must be able to see.
   Cheapest, and it needs a stated number rather than a guess — the lease is 60 s,
   so a window under that keeps 0906 invisible.
3. **Leave the window and state the bound.** Legitimate if the cell is only ever
   meant to prove first-delivery, but then something else must cover session
   lifetime, and today nothing does.

Whichever lands, the acceptance is the counterfactual: build with
`Z_TRANSPORT_LEASE_MS = 10_000` and require the cell to FAIL.

## Not covered

Whether the same run-to-completion shape is aimed at other free-running
producers elsewhere in the harness. `wait_for_output` has other callers and they
have not been swept.
