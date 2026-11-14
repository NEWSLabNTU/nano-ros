---
id: 2535
title: "RFC-0073's three open questions on the platform clock are unanswered"
status: open
type: enhancement
severity: low
area: [platform, executor]
related: [rfc-0073, phase-352]
found: 2026-11-14
---

**Home: [RFC-0073](../design/0073-platform-clock-nanoseconds-and-resolution.md)
"Open questions"**

## What is open

phase-352 shipped the interface whole (`nros_platform_clock_ns` and
`nros_platform_clock_resolution_ns`). The RFC still lists three questions
that nobody has measured or ruled:

1. Is the executor's per-spin clock read frequent enough to justify the
   deferred coarse variant? Measurable on the existing lane.
2. May `resolution_ns` change after init for a port with a scaling clock,
   instead of the conservative worst-case rule?
3. The instruction-count measurement repeated on silicon. QEMU `icount`
   counts instructions, not cycles, so the RFC's absolute microseconds hold
   only under QEMU.

## Found by

phase-555's design review (W3, platform-board-rtos brief P4). The user ruled
(2026-11-14) that an RFC becomes Stable only after its open questions are
filed as issues and the RFC points at them.

## What closes it

Each question answered in RFC-0073 with its evidence (a measurement for 1 and
3, a ruling for 2), or dropped there with the reason.
