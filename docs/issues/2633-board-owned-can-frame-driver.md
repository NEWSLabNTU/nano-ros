---
id: 2633
title: "No platform CAN frame interface: an application sending its own CAN / CAN FD frames picks a backend per platform with #if"
status: open
type: enhancement
severity: medium
area: [platform, zephyr, freertos, boards]
related: [rfc-0132, phase-560, 2573, rfc-0109, rfc-0080]
found: 2026-11-16
---

GitHub: NEWSLabNTU/nano-ros#4012

**Home: [phase-560](../roadmap/phase-560-contract-revision-follow.md) W7**,
beside issue [2573](2573-can-fd-e2e-profile-and-flexcan-helper.md) (GitHub
#3761): this is the portable layer under 2573's FlexCAN helper. Design:
[RFC-0132](../design/0132-board-can-frame-driver.md) (Draft, review-gated).

## What is missing

The platform ABI has no CAN frame interface (`packages/drivers/` holds `ipc/`,
`net/`, `serial/`; the zenoh-pico CAN link carries the RMW, not application
frames). ASI sends control commands on CAN / CAN FD and chooses its backend in
`actuation_module/src/asi_common/include/platform/platform_can.h`:

- `zephyr_can.h` (`DEVICE_DT_GET(DT_CHOSEN(zephyr_canbus))`),
- `zephyr_can_udp_tunnel.h` (the FVP has no CAN),
- `freertos_can_socketcan.h` (POSIX; interface from
  `getenv("SAFETY_ISLAND_CAN_IFACE")`, format from `SAFETY_ISLAND_CAN_FORMAT`),
- `freertos_can_mock.h`, `freertos_can_stub.h`.

The chain needs `PLATFORM_*` defines, which is the main reason `asi_common`
still has them, and why the census cannot build a host image (ASI keeps
`[census] on_missing = "warn"`).

## Wanted

A board-owned CAN frame driver (`open`, `send`, `send_fd`, `fd_active`,
optional `recv`) selected per board or image, with the device named by a
board or image fact. The application keeps its encoder and frame layout.

## Plan

RFC-0132: one C ABI `<nros/platform_can.h>` + a `PlatformCan` trait; backends
`zephyr` (devicetree), `socketcan` (Linux), `board` (vendor driver through the
board package) and `mock` (host tests); `[board.can]` in the descriptor and an
`[image.<id>] can` row, carried like the other board facts (never `getenv`).
Implementation waits for the RFC review.
