---
id: 2522
title: "The CAN links have never run on CAN hardware, and no open phase plans it"
status: open
type: test-gap
severity: medium
area: [rmw-zenoh, testing, boards]
related: [phase-377, phase-394, phase-555]
found: 2026-11-14
---

**Home: the zenoh-pico CAN links (RFC-0080 multicast, RFC-0083 ISO-TP unicast)**

## What is missing

Both CAN links were built and tested without CAN hardware: vcan on Linux,
and Zephyr's own simulated CAN. The hardware step was planned in both
phases and closed with neither:

- phase-377 (archived) left **W6** open: "Hardware: MR-CANHUBK344 CAN ↔
  USB-CAN dongle ↔ Linux peer. Real bit rates, real errors, real timing."
  Its status line ends "Remaining: W6 (hardware)."
- phase-394 (archived) left **tier 3**, MR-CANHUBK344 to a Linux host,
  "still untouched", and says twice that tier 3 is "the only thing that
  closes" its open behaviour questions.
- phase-512, the open on-hardware phase, runs the CAN hub board over SERIAL
  only. It has no CAN item.

So no open phase or issue plans the measurement that would show either link
working at a real bit rate, under real bus errors and arbitration with other
traffic.

## Found by

phase-555's design review (rmw-wire brief, "Untracked work found"). The user
ruled that the untracked items be filed as issues (2026-11-14).

## What closes it

A work item in an open phase (phase-512 is the natural home) that runs the
MR-CANHUBK344 over CAN against a Linux peer, for both the multicast link and
the ISO-TP unicast link, and records the result in RFC-0080 and RFC-0083.
