---
id: 2022
title: "A bare-metal image busy-polls its transport between packets: a tickless idle in the smoltcp loop needs a LAN9118 receive interrupt first"
status: resolved
type: limitation
area: [platform, drivers, executor]
severity: low
found: 2026-10-16
resolved: 2026-10-24
related: [phase-436]
---

**Home: none** — stands on its own since 2026-10-24, when
[phase-436](../../roadmap/archived/phase-436-poll-wake-revision-deadline-driven-executor.md)
was archived. This is that phase's item C3, deferred there on 2026-10-16.

## What

Every bare-metal backend is poll-only: zenoh-pico over smoltcp and XRCE both
read the transport only inside `drive_io`. RFC-0106 D5 therefore keeps the
receive-blocking wait on bare metal whatever park is installed, so the executor
never parks there, and a park primitive installed on mps2-an385 would never be
called.

The wait that does run is the transport's: `nros_smoltcp`'s poll loop. It
already idles on `wfi` through `set_idle_callback` when an interrupt source is
armed (the RTIC entry, `enable_wfi_idle`). Otherwise it busy-polls until the
wait's deadline.

## What would fix it

A tickless idle inside that loop: arm a one-shot compare at the wait's deadline
and `wfi` until either the compare or a packet arrives.

The precondition is a receive interrupt in `lan9118-smoltcp`, which has none.
The FreeRTOS lwIP driver's `lan9118_lwip_rx_irq_enable` is the model. Without
it, a `wfi` armed only on the deadline is the blind sleep D5 exists to prevent,
moved down one layer: a packet arriving mid-sleep waits for the compare.

## Why low

The gain is power, not latency. The busy-poll answers a packet as fast as the
loop turns, and jitter reporting is not affected: bare metal keeps the 1 ms
granularity of the `wake_wait_ms` ABI it exports, which is not an over-claim
(phase-436 Path C exit).

## Resolution (2026-10-24)

Done on mps2-an385, the one bare-metal board with a LAN9118.

- **Receive interrupt.** `lan9118_smoltcp::irq::{unmask_rx, mask_rx,
  rx_frames_pending}`, modelled on `lan9118_lwip_rx_irq_*` (issue 0917). RSFL
  is a level, so the handler masks and the drain side unmasks. They are free
  functions over the register base because the handler runs while the poll
  may hold the driver mutably. `Lan9118::rx_buffered` reports the one frame
  `receive` holds back while the TX FIFO is full, which an empty FIFO does not
  rule out.
- **The seam.** `nros_smoltcp::wait_idle`: a board installs a `WaitIdleFn`,
  and a wait loop calls `idle_until_ms(deadline)` AFTER it has polled and
  found nothing. The deadline is narrowed by smoltcp's own `poll_delay`
  (retransmit, delayed ACK, ARP), and an overdue stack does not sleep. It is
  not `do_poll`'s idle hook: that runs before the loop looks at what the poll
  delivered, so a tickless sleep there would hold a reply that had already
  arrived. Callers: `zpico_spin_once`'s smoltcp read loop (through the C
  export `nros_smoltcp_idle_until_ms`) and the six loops in
  `define_smoltcp_platform!` (TCP connect, TCP/UDP `read_exact` and `send`,
  UDP `read`). With no primitive installed the call returns at once, so
  stm32f4 and esp32 busy-poll as before.
- **The primitive.** `nros-board-mps2-an385`'s `tickless` module: the
  `ETHERNET` handler (NVIC 13) and a SysTick one-shot. SysTick is the one
  counter on the board no one else owns (issue 1804's table). The check, the
  unmask, the timer and the `wfi` run under PRIMASK, so a frame landing
  between the check and the sleep ends the sleep. Not built for RTIC images,
  whose dual-timer tick already wakes their `wfi`.

**Measured** on QEMU (`-icount shift=auto`), an idle `qemu-bsp-listener`
connected to a router with no talker. Host CPU of the QEMU process over 20 s,
the same image with and without `tickless::install()`, two runs each:
99.9 % / 99.9 % busy-polling, 1.4 % / 0.9 % tickless.

**Still green with it:** `test_qemu_bsp_pubsub_e2e`, all six
`test_qemu_baremetal_capi_e2e` cases (C/C++ pubsub, service, action — the
`zpico.c` loop) and, as the RTIC control, `test_qemu_rtic_{pubsub,service}_e2e`.
