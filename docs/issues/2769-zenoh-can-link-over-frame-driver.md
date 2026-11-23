---
id: 2769
title: "The zenoh-pico CAN link does not ride the platform CAN frame driver: no arm on a board-stack board, no shared device owner"
status: open
type: enhancement
severity: low
area: [rmw-wire, platform, boards]
related: [rfc-0080, rfc-0083, rfc-0132, 2633, phase-560, phase-478]
found: 2026-11-20
---

GitHub: NEWSLabNTU/nano-ros#4456

Filed by phase-560 W9.f as RFC-0132 open question 4's follow-up (issue
[2633](2633-board-owned-can-frame-driver.md)).

**Tracked by:** [phase-478](../roadmap/phase-478-island-developer-experience-findings.md) I48, owner the ASI port session (claimed 2026-11-23).

## What is missing

The zenoh-pico CAN and ISO-TP links (RFC-0080, RFC-0083) open their device
through zenoh-pico's own platform arms, `ZENOH_LINUX` and `ZENOH_ZEPHYR`
(`just check zenoh-port-links` derives the list from the `_z_open_<link>`
guards). The platform CAN frame driver `<nros/platform_can.h>` (RFC-0132)
reaches the same devices on more boards: on a `"board"`-stack board its
backend is the board package's strong symbols over a vendor driver. So:

- On a `"board"`-stack board the RMW cannot ride CAN at all: the link has no
  arm there, and `nros configure` refuses the locator (`link_arms.rs`).
- Where a vendor driver cannot share a controller, nothing arbitrates between
  the link and an application's own frames. RFC-0132 refuses an image whose
  RMW wiring and frame driver name the same `[canN]`; it does not share.

## Wanted

A zenoh-pico CAN link arm over `nros_platform_can_open` / `send` / `recv`, so
every board with a frame driver can carry the RMW on CAN and the link and the
application share one device owner. The link reads, so it is the first
consumer of a receive path: RFC-0132 open question 2 deferred a readiness
callback from `recv` into the executor (RFC-0084) to that consumer.

## Owner

The RFC-0080 / RFC-0083 link work, not RFC-0132 (open question 4).
