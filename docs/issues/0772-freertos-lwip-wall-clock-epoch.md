---
id: 772
title: "FreeRTOS/lwIP has no wall-clock epoch — the same consumer that drove the Zephyr one runs a FreeRTOS board too"
status: open
type: enhancement
area: platform, boards
related: [issue-0758, phase-372, phase-292]
---

## What is missing

`nros_platform_epoch_us()` (issue 0758 W1) is implemented on POSIX/NuttX
(`CLOCK_REALTIME`) and on Zephyr (SNTP, W2–W4). The FreeRTOS port returns `0` —
"this image has no wall clock" — so an image on that family stamps messages from
its boot epoch, and a peer that validates stamps rejects them.

That is the same defect the Zephyr work fixed, on the other family.

## The demand is named, and it is the same consumer

0758 was driven by the autoware-safety-island: it stamped control commands from
its boot epoch and Autoware's `vehicle_cmd_gate` rejected them as stale, so
autonomous mode could never actuate until ASI added SNTP consumer-side.

That island's Zephyr target is `fvp-aemv8r-smp` — but ASI is a consumer of the
FreeRTOS board too. `packages/boards/board-support.toml:101`, on
`nros-board-s32z270-freertos`:

> S32Z270 RTU Cortex-R52 (**ASI phase-4 W5.b consumer**). No matrix_platform:
> no witness exists for this board […]

So the FreeRTOS half is not speculative coverage. 0758's own closing note said
"no FreeRTOS consumer has asked" — that was read off the Zephyr side alone and
is wrong: the same consumer runs both, and will hit the same
rejected-stamps failure on the R52 board.

## The shape is NOT a port of the Zephyr code

Checked in 0758 (see its "FreeRTOS / lwIP checked" section) — copying the Zephyr
implementation by analogy will produce the wrong thing:

* **Available.** `third-party/freertos/lwip/src/apps/sntp/sntp.c` is vendored.
  Our build does not compile it: `nros-board-freertos/build.rs` carries an
  EXPLICIT lwIP source list (core, ipv4+igmp, api/sockets, netif, FreeRTOS
  `sys_arch`) with no `src/apps/*`. Adding it is one line plus `SNTP_*` defines
  in `lwipopts.h`.
* **Asynchronous, with no synchronous one-shot.** Zephyr's
  `sntp_simple(server, timeout, &ts)` BLOCKS and returns the time, which is why
  0758 W2 could acquire an offset inline at boot. lwIP has no such call —
  `grep -c 'sntp_simple\|sntp_request_sync'` on its `sntp.c` is 0. It is a
  background daemon (`sntp_setservername` + `sntp_init`, returning immediately)
  delivering time through a COMPILE-TIME macro the port defines:
  `SNTP_SET_SYSTEM_TIME(sec)`, or `SNTP_SET_SYSTEM_TIME_US(sec, us)` for the
  precision this ABI wants.

## What that costs, and the decision it forces

The ABI absorbs the asynchrony natively — the returns-0 sentinel already means
"not acquired yet", so `epoch_us()` answers 0 until the first callback and
non-zero after. Not stalling boot on a network round trip is arguably better
than Zephyr's blocking acquire.

What does NOT carry over is a GUARANTEE. On Zephyr the epoch is acquired between
netif-up and the first component (0758 W4), so no message is ever stamped
boot-relative. With lwIP the time lands whenever the daemon gets a reply, so
early messages carry boot-relative stamps and the clock flips mid-run — the
original ASI failure reappearing as a startup transient rather than a permanent
one. Whoever takes this must decide, and say so in the port's documentation:

* tolerate the opening window, or
* gate publishing on `epoch_us() != 0`, or
* block at boot for the first callback with a timeout, recovering Zephyr's
  guarantee at the cost of Zephyr's boot stall.

## Verification is hardware-only, and that is the real cost

`board-support.toml` calls S32Z270 a hardware-only board: "no emulator models
the S32Z270 RTU". There is no matrix_platform and no witness, and its
link-completeness is proven only by the workspace cmake lane.

So unlike the Zephyr half — which was proven end-to-end on native_sim against a
real NTP server, degraded path and success path both — this one cannot be
runtime-proven in CI as the tree stands. A `mps2-an385` lwIP image under QEMU
could prove the ACQUISITION path generically (it has lwIP and a slirp host), and
that is probably the honest bar: prove the mechanism where an emulator exists,
and leave the R52 board's proof to the consumer who has the hardware.

Do not let that gap turn into a cell that asserts a marker no CI image ever
prints.

## Acceptance

* `nros_platform_epoch_us()` returns absolute time on a FreeRTOS image once
  lwIP's SNTP has answered, and `0` before — verified by RUNNING an image, not
  by reading the port.
* The startup-window semantics above are chosen deliberately and documented in
  the port, not left for a consumer to discover at a `vehicle_cmd_gate`.
* No test asserts a marker that no CI-reachable image emits.
