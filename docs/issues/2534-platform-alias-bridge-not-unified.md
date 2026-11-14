---
id: 2534
title: "Two `platform_aliases.c` bridges remain, and zenoh-pico's `z_clock` still bypasses the platform ABI"
status: open
type: tech-debt
severity: low
area: [platform, rmw-zenoh, rmw]
related: [rfc-0034, phase-230]
found: 2026-11-14
---

**Home: [RFC-0034](../design/0034-platform-layer-split.md) D3 ("one bridge,
platform-owned, category-gated")**

## What is open

RFC-0034 D1 to D8 have landed except two parts of D3:

- **One bridge.** D3 says the `z_* → nros_platform_*` forwarder moves to a
  single platform-layer artifact that both zenoh-pico and XRCE consume. Two
  copies are still tracked:
  `packages/rmw/zenoh/zpico-sys/c/zpico/platform_aliases.c` and
  `packages/rmw/xrce/nros-rmw-xrce/src/platform_aliases.c`.
- **`z_clock` / `z_time`.** The scalar funnel covers alloc, sleep and random.
  The clock stays vendored, because `z_clock_t` is not portable (a struct on
  ThreadX, `TickType_t` on the FreeRTOS host port, `uint64_t` on bare metal).
  The zpico `platform_aliases.c` says so in its own comment. RFC-0034's
  "Implementation status" lists it as deferred.

Neither is a correctness bug today. The D8 lint (`check-no-direct-kernel-alloc`)
does not scan the vendored submodule, so nothing fails while they stay open.

## Found by

phase-555's design review (W2, platform-board-rtos brief P3). The user ruled
(2026-11-14) that RFC-0034 becomes Stable once its open items are filed as
issues and the RFC points at them. This is that issue.

## What closes it

- One alias TU, owned by the platform layer, linked by both zpico-sys and
  nros-rmw-xrce; the second copy deleted.
- `z_clock_*` routed through `nros_platform_clock_ns` (RFC-0073) behind a fork
  guard, or a written decision in RFC-0034 that the clock stays vendored, with
  the reason.
