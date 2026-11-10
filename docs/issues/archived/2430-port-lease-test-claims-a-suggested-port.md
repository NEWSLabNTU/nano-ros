---
id: 2430
title: "`a_named_port_waits_for_its_holder_and_is_handed_over` claims a suggested port another test may already hold"
status: resolved
type: bug
area: [testing]
severity: low
found: 2026-11-11
related: [2125, phase-535]
---

## What happens

`just ci gate`'s `test-lane-contracts` step failed once with

    port_lease::tests::a_named_port_waits_for_its_holder_and_is_handed_over
    panicked at packages/testing/nros-tests/src/port_lease.rs: first claim wins

on a change that does not touch `port_lease.rs`.

## Why

The test took a port from `Transport::suggest()` (an ephemeral port the OS
had just freed) and asserted that its first `claim` on that port's lock
wins. A lease holds only the lock file, never the socket, so the OS hands
the same ephemeral port to the next `bind(…:0)`. A test running beside this
one in the same nextest run that holds a lease on that port makes the claim
lose. `lease_port` already retries for exactly this reason; the test did
not use it.

## Resolution

The test leases its port with `lease_port` and plays the foreign holder with
that lease, so the start race is gone rather than retried.
