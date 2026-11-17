---
id: 2668
title: "`a_named_port_held_past_the_budget_fails_naming_the_holder` claims a suggested port another test may already hold"
status: resolved
type: bug
area: [testing]
severity: low
found: 2026-11-17
resolved: 2026-11-17
resolved_in: "the change that filed it"
related: [2430, 2125, phase-535]
---

## What happens

`just ci gate`'s `test-lane-contracts` step failed once, at load average about
15, with

    port_lease::tests::a_named_port_held_past_the_budget_fails_naming_the_holder
    panicked at packages/testing/nros-tests/src/port_lease.rs:619:51:
    first claim wins

on a change that does not touch `port_lease.rs`. Run alone straight after, it
passed 5 of 5.

## Why

This is [issue 2430](2430-port-lease-test-claims-a-suggested-port.md) in the
sibling test that 2430's fix did not reach. The test took a port from
`Transport::suggest()` and asserted that its first bare `claim` on that port's
lock wins. A lease holds only the lock file, never the socket, so the OS can
hand the same ephemeral port to a test running beside this one in the same
nextest run. If that test holds a lease on it, the claim loses.

Issue 2430 fixed `a_named_port_waits_for_its_holder_and_is_handed_over` by
leasing its port with `lease_port`, which retries for exactly this reason. The
budget test, which follows it in the same file, kept the bare claim.

Sweep: `grep -n 'suggest().expect' packages/testing/nros-tests/src/port_lease.rs`
finds no other site. The remaining bare `claim(...).expect("first claim wins")`
calls run on `probe(...)` paths private to their test, so nothing else can
hold them.

## Resolution

The budget test leases its port with `lease_port` and plays the foreign holder
with that lease, as the hand-over test does.
