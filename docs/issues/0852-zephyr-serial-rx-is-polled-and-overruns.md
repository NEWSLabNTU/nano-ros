---
id: 852
title: "zenoh-pico's Zephyr serial RX is polled with no interrupt buffering and no
  error check, so it silently drops bytes under load"
status: open
type: bug
area: rmw
related: [issue-0848, issue-0839]
---

## Problem

`_z_read_serial_internal` (`src/system/zephyr/network.c`) receives byte by byte:

```c
res = uart_poll_in(sock._serial, &raw_buf[i]);
if (res != 0) { if (past deadline) return SIZE_MAX; k_yield(); }
```

- `CONFIG_UART_INTERRUPT_DRIVEN` is **not set** on the board images
- the port calls **only** `uart_poll_in` — no `uart_fifo_read`, no async/DMA
- `uart_err_check` was **never called**, so overruns were invisible

The S32K344 LPUART has a small RX FIFO and a byte arrives every 87 us at
115200. Any scheduling delay between polls loses bytes, and the loss is not
reported.

## Proof

Instrumented the read loop to report `uart_err_check` on both paths, against a
locally built zenoh router that logs every serial write:

```
DIAG-RX: frame rb=9  ok hdr=0x03  uart_err=0x0     <- handshake, board idle
DIAG-RX: frame rb=83 ok hdr=0x00  uart_err=0x0
DIAG-RX: frame rb=15 ok hdr=0x00  uart_err=0x0
DIAG-RX: timeout, rb=4  uart_err=0x1 OVERRUN       <- keepalive frame, truncated
DIAG-RX: timeout, rb=0  uart_err=0x0  x many       <- nothing left to lose
```

The overrun flag is set on **exactly** the truncated frame and nowhere else.

## Why it looks load-dependent

- **handshake survives** — the board is otherwise idle and the poll loop keeps up
- **keepalives die** — by then three queryables, two publishers and their
  callbacks are competing for the CPU
- **the talker soaks for five minutes** — far lighter load, poll loop keeps up

That is the whole reason this presented as "actions are broken and pub/sub is
fine".

## The red herring worth recording

[Issue 0848](archived/0848-router-sends-no-keepalives-on-serial.md) chased this as a
router defect for a long time, ending on "the keepalive is a 1-byte write that
never frames". The 1-byte figure was the payload handed to the link; z-serial
frames it as `header(1) + len(2) + payload + crc32(4)` and COBS-encodes it, so
~10 bytes reach the wire. The board caught 4 of them and overran. **The small
write was never the anomaly — the receiver was.**

The router is exonerated: its timer fires, the keepalive arm fires,
`write_all` + `flush` both succeed, and the frames it emits are well formed.

## Fix direction

`CONFIG_UART_INTERRUPT_DRIVEN=y` plus `uart_fifo_read` in an ISR feeding a ring
buffer, or the async API with DMA. Either way bytes are buffered by hardware or
the driver instead of depending on thread scheduling. This is a real change to
the port — the read path is written around `uart_poll_in` and returns
`SIZE_MAX` on a per-byte deadline — not a config flip.

**Cheap interim step regardless of the above:** call `uart_err_check` and
surface overruns. They are currently invisible, which is what let this hide
behind six other hypotheses.

## Impact

Any zenoh-over-serial image whose CPU is busy enough to miss an 87 us polling
window loses frames silently. Observed as session expiry at
`2 x Z_TRANSPORT_LEASE` ([issue 0839](0839-action-image-session-expires-every-20s.md)),
because the dropped frames are the router's keepalives.
