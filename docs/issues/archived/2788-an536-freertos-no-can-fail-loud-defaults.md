---
id: 2788
title: "mps3-an536-freertos has no CAN backend and does not compile the fail-loud defaults, so an image that calls the CAN driver does not link"
status: resolved
type: bug
severity: low
area: [boards, freertos]
related: [phase-560, issue-2633]
found: 2026-11-21
resolved: 2026-11-22
resolved_in: "fix(can): the metadata probe links the census CAN mock; every board without a CAN backend gets the fail-loud defaults (2787, 2788)"
---

**Home: [phase-560](../../roadmap/phase-560-contract-revision-follow.md) W9.f.**

## What happens

W9.f's weak fail-loud defaults (`nros-board-common/c/nros_board_can_unsupported.c`:
`open` returns ENOTSUP and prints a line naming the board) are compiled on
`s32z270-freertos` only. `mps3-an536-freertos` has no CAN backend either, and
does not compile them, so a consumer image that calls `nros_platform_can_*`
fails to link there instead of getting the ENOTSUP default at run time. The
safety island has to compile its CAN output out of the AN536 image with a
cmake argument.

## Wanted

Every board with no CAN backend compiles the fail-loud defaults (or the
check that lists which boards do is extended to AN536), so the same
application links on all of them and fails loud only when it opens CAN.

## Acceptance

`just check can-board-defaults` covers mps3-an536-freertos, and an AN536
image that calls the driver links and gets ENOTSUP from `open`.

## Resolved, 2026-11-22

The defaults moved from one board's TU to two shared places, one per road,
so every board without a CAN backend gets them and no board has to opt in:

- **cmake road:** `nano_ros_entry` calls `nros_entry_can_defaults(<entry>
  <board>)` (`cmake/NanoRosCanImage.cmake`) for every entry on a cross
  board. When the board's stack is not "host", it compiles a per-entry TU
  that names the board and includes `nros_board_can_unsupported.c`, an
  object of the image, so a strong driver in any other object wins.
- **cargo road:** `nros_board_common::can_unsupported::add_defaults`, called
  by the FreeRTOS glue runner (`freertos_build::run_overlay`: mps2-an385-freertos,
  mps3-an536-freertos, s32z270-freertos) and the ThreadX RISC-V one.

A host stack (SocketCAN) is never given them, and a Zephyr entry never
reaches the rule (its backend is `nros-platform-zephyr`). The sweep found the
same gap on every board that is neither: mps2-an385-freertos, mps2-an385
(bare metal), rv-virt-threadx and NuttX had no defaults either. All are
covered on the cmake road. mps2-an385 and nuttx-qemu compile no board C glue
on the cargo road, so a Rust image calling the driver there still needs a
strong driver; the check lists them by name.

`s32z270-freertos`'s own `c/can_unsupported.c` is gone: the shared TU names
the board the same way.

`just check can-board-defaults` holds it: an AN536 entry that calls the
driver links and `open` returns ENOTSUP with the board's name; a strong
`open` in another object wins; a host-stack board gets no defaults (negative
control); every in-tree board that is neither host nor Zephyr is covered.
Rows A to D fail on the parent commit.
