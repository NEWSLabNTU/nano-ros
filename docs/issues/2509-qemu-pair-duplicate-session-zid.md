---
id: 2509
title: "Two QEMU instances of a NuttX image can draw the same zenoh session ZID, so the router refuses the second and the pair never talks"
status: open
type: bug
severity: medium
area: [rmw-zenoh, nuttx, zephyr, testing]
found: 2026-11-13
related: [2503]
---

## What

Measured on 2026-11-13 during a tier-2 run: `rtos_e2e`
`test_rtos_service_e2e` on NuttX (C) failed with "got 0 responses" after
about 3 minutes, on main `8b07c45633` as well (in-lane 2 pass / 1 fail;
manual pairs 4 of 8 failed). In every failure the client opened its zenoh
session with the server's ZID. The router refuses a session whose ZID is
already connected, so only the server's transport opened, the client kept
reconnecting (`z_liveliness_declare_token failed: -100`, then `-103`) and
never got a reply. About 40-50 % of runs fail.

## Why

On NuttX, `zpico_fill_session_zid` (`zpico.c`, generic branch) seeds the
ZID from the session object's address (the same in both ELFs), a counter
(0 in both) and `CLOCK_REALTIME`, which with no RTC is the 1 ms tick from
boot. Under `-icount shift=auto` two instances boot identically and often
open their session in the same millisecond. The platform RNG is a
fixed-seed xorshift and adds nothing. The ThreadX rv-virt pair had the same
problem and was fixed by mixing in the MAC; NuttX was left out.

## Sweep of every port (2026-11-13)

- Linux, freertos-posix, threadx-linux, PX4 SITL: `getrandom` first. OK.
- Bare-metal mps2-an385 (smoltcp): RNG seeded from the image IP; serial
  from semihosting host time. OK.
- FreeRTOS (mps2-an385, mps3-an536, s32z270): RNG seeded from IP + MAC;
  pairs state distinct hosts and derived MACs. OK.
- ThreadX riscv64: IP/MAC + device MAC + CLINT; launcher varies the MAC. OK.
- PX4 hardware: a NuttX build, covered by the NuttX fix.
- ESP32: dormant.
- Zephyr on QEMU (mps2/an385): no per-instance input either (test RNG,
  cycle counter, address), but measured 8/8 concurrent pairs drew distinct
  ZIDs (icount idle warp), and no in-tree test boots two Zephyr QEMU guests.
  Hardened anyway.

## Fix

- NuttX: mix `eth0`'s MAC (`SIOCGIFHWADDR`) into the seed; the test gives
  each NuttX QEMU peer its own MAC (`QemuProcess::start_nuttx_virt_peer`).
- Zephyr: mix the default interface's link-layer and IPv4 addresses into
  the seed (both Zephyr lines).
- Either falls back to the old seed when the fact is unavailable.

Measured: NuttX pairs with distinct MACs 10/10 answered; same MAC with the
fix 3/6 failed (the MAC is what matters); in-lane 6/6.

## Acceptance

The NuttX service/pubsub/action pairs pass repeatedly in the tier-2 lane;
two instances of one image never share a ZID when their MAC or IP differs.
