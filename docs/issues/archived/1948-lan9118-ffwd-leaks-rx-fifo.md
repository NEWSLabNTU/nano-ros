---
id: 1948
title: "QEMU's LAN9118 fast-forward leaks RX FIFO space, so an mps2-an385 FreeRTOS island stops receiving FOR GOOD once a flood drains its pbuf pool"
status: resolved
type: bug
area: [drivers, network, qemu, freertos]
severity: high
found: 2026-10-18
related: [506, 1949, phase-462, rfc-0074]
---

## Measured

The phase-462 W6a flood-cell rerun (nano-ros-rt-eval, FreeRTOS
mps2-an385, QEMU 11.0.0-nros6, `-nic user,model=lan9118`), once the load
generators were actually running (issue 1949). Per flood level, both RFC-0074
cells (`unbounded`, `NROS_ZENOH_RX_TASK_BUDGET_FRAMES=8`):

| flood | ctrl ticks / ~33 s | stalls > 50 ms | inbound rx/s |
|---|---|---|---|
| none | 3,309 | 0 | 10 (the peer) |
| 500 Hz | 3,292 / 3,305 | 0 / 0 | ~480 / ~480 |
| 1 kHz | 3,294 / 3,295 | 0 / 0 | ~805 / ~880 |
| 1.5 kHz | 542 / 524 | 304 / 307 | 200 -> **0** / 24 -> **0** |
| 2 kHz | 522 / 518 | ~300 | 462 -> 45 -> **0** |

Above ~1 kHz the island does not stall and recover: within two seconds it
receives NOTHING again, the peer's 10 Hz commands included, and every tier
runs on 100 ms gaps for the rest of the run. The budget made no difference.

Sampled through the QEMU gdbstub in that state:

- the CPU sits in `vApplicationIdleHook` (18 of 20 samples) — the island is
  not starved, it is waiting; 100 ms is the socket timeout its sends block on
  once no ACK can arrive;
- `RX_FIFO_INF = 0x0000015c`: **zero** status entries but **348 bytes** in
  the data FIFO, constant from t ~ 14 s to the end — data with no packet.

## Why

`hw/net/lan9118.c`, `CSR_RX_DP_CTRL` (fast-forward), in our QEMU fork
(`third-party/qemu/qemu`, 729262e975) and upstream through 11.0:

```c
s->rx_fifo_head += s->rxp_size;      /* head advances ...               */
if (s->rx_fifo_head >= s->rx_fifo_size)
    s->rx_fifo_head -= s->rx_fifo_size;
/* ... but rx_fifo_used is never reduced by rxp_size, and rxp_size is
 * never cleared, so the next pop continues the skipped packet. */
```

Every fast-forward therefore leaks the skipped packet's words from the free
space `lan9118_receive` checks, and leaves the pop state mid-packet. The
guest fast-forwards whenever it drops a frame, and `lan9118-lwip` drops one
whenever `pbuf_alloc(PBUF_POOL)` fails — which is exactly what a flood above
the island's drain rate causes. Each dry-pool drop leaks; once the leak
reaches the FIFO's size, `lan9118_receive` refuses every frame and slirp
discards it. Nothing reclaims the space, so it is permanent.

This also explains why the cliff sits between 1 kHz and 1.5 kHz: below the
drain rate the pool never runs dry and no fast-forward happens at all.

## Resolution (2026-10-18)

Both LAN9118 drivers drop a packet of known length by READING its words
(`rx_drain_words`), which is correct on silicon as well and never touches
the leaking path. Fast-forward stays only for a length that cannot be
trusted (outside `4..=MAX_FRAME_SIZE`), where there is nothing to count.

- `lan9118-lwip` (mps2-an385 and mps3-an536 FreeRTOS): the dry-pool drop and
  the error-status drop.
- `lan9118-smoltcp` (bare-metal mps2-an385): the error-status drop. It has no
  allocation failure, so it rarely reached this path; same class, same fix.

The QEMU model is still wrong, and fixing it belongs in our fork's
`nano-ros-v11.0.0-patches` line (`rx_fifo_used -= rxp_size; rxp_size = 0;`
in the fast-forward arm), with a QEMU SDK re-cut — not done here.
