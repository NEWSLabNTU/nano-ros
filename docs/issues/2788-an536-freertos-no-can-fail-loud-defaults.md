---
id: 2788
title: "mps3-an536-freertos has no CAN backend and does not compile the fail-loud defaults, so an image that calls the CAN driver does not link"
status: open
type: bug
severity: low
area: [boards, freertos]
related: [phase-560, issue-2633]
found: 2026-11-21
---

**Home: [phase-560](../roadmap/phase-560-contract-revision-follow.md) W9.f.**

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
