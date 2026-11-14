---
id: 2573
title: "The E2E path is zenoh-only: a CAN FD link needs a fixed-length E2E profile and a raw FlexCAN TX/RX helper on Zephyr"
status: open
type: enhancement
area: [rmw, zephyr, boards, safety]
severity: medium
found: 2026-11-13
related: [phase-560, rfc-0028, 2487, 2488]
---

GitHub: NEWSLabNTU/nano-ros#3761

**Home: [phase-560](../roadmap/phase-560-contract-revision-follow.md)** W7,
whose end-to-end protection addition (T7) is the image's to check; the FlexCAN
helper is board work that rides with it. L4 plan item NR-6 (exit criterion
FC8). Usable on the K344 now.

## What is needed

The L4 island sends the actuator command (I16) and the island-alive signal to
the external supervisor (CANHUB) over CAN FD, not zenoh. These links need E2E
protection in a fixed-length frame:

- CRC-16 or CRC-32;
- a sequence counter;
- a Data ID.

They also need a small raw FlexCAN TX/RX helper on Zephyr for the S32K3 (and
later the S32Z). The existing E2E path is zenoh-only (issue 2487 makes it fail
closed on other backends).
