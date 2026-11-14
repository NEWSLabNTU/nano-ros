---
id: 2543
title: "The smoltcp deadline source of the executor park has run only on QEMU, never on hardware"
status: open
type: test-gap
severity: low
area: [executor, baremetal, smoltcp, hardware]
related: [rfc-0106, phase-555]
found: 2026-11-14
---

## What

RFC-0106 D1 bounds the executor's park by the minimum of its declared
deadlines. smoltcp's `poll_delay` is the only port deadline source in the
tree (mps2-an385, and esp32-qemu while ESP32 was live). It wins a park on
QEMU when the budget leaves it room (phase-436 Path B's exit). RFC-0106's
open question "Deadline sources on hardware" records that no hardware run
has been made.

## Why it is filed now

Phase-555 W3 flipped RFC-0106 to Stable, and an open question cannot stay
only in the RFC. It needs a board with a smoltcp transport and a way to
read the park's wake reason on target.

## Acceptance

One run on a hardware board with a smoltcp transport shows a park ended by
the smoltcp deadline (not by the caller's budget or a timer), with the wake
reason read from the target, recorded in RFC-0106's coverage table.
