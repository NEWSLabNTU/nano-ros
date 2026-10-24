---
id: 2022
title: "A bare-metal image busy-polls its transport between packets: a tickless idle in the smoltcp loop needs a LAN9118 receive interrupt first"
status: open
type: limitation
area: [platform, drivers, executor]
severity: low
found: 2026-10-16
related: [phase-436]
---

**Home: none** — stands on its own since 2026-10-24, when
[phase-436](../roadmap/archived/phase-436-poll-wake-revision-deadline-driven-executor.md)
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
