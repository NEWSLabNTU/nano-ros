---
id: 1534
title: "On Zephyr the tx-flush task runs above the read task, and on a serial link its busy-wait starves the RX drain"
status: open
area: zenoh, zephyr, serial, scheduling
severity: medium
phases: [279, 282]
rfcs: []
related: [0626, 0623, 0852, 1533]
---

# On Zephyr the tx-flush task runs above the read task

Issue 0626 gave the zenoh-pico READ and LEASE tasks a stated priority on Zephyr
(`CONFIG_NROS_ZENOH_READ_PRIORITY` / `_LEASE_PRIORITY`, band 200, which is
`k_thread` 4 on a `CONFIG_NUM_PREEMPT_PRIORITIES=15` image), applied in
`zpico_open` when no board called `zpico_set_task_config`. The tx-flush task of
phase-279/282 (`_zpico_tx_flush_task_fn`, `ZPICO_TX_BATCH_THREAD`) has the same
setter, `zpico_set_flush_task_config`, and no Zephyr caller and no default: it is
spawned with a NULL attribute and lands at the platform default, `k_thread` 0.

So the thread that TRANSMITS outranks the thread that RECEIVES, the reverse of
the ordering the READ_PRIORITY help text says was chosen on purpose.

## Why it matters on a serial link

The Zephyr serial link transmits with `uart_poll_out` per byte, a busy-wait
(86.8 us per byte at 115200, 10.9 us at 921600). A flush of a batch holds the
CPU for the whole batch at `k_thread` 0; the read task at `k_thread` 4 cannot
run, and the ISR keeps filling the 1 KiB RX ring (issue 0852) until it
overflows. The main thread (`k_thread` 0) does the same while it sends the
image's declarations at registration.

Measured on the safety island (S32K344, 115200 baud, SWD reads while it runs):

    thread               prio   (read from k_thread.base.prio)
    <tx flush>              0   entry _zpico_tx_flush_task_fn, arg g_sessions
    zpico_lease             4
    zpico_read              4
    main                    0   during registration

- the tx-flush thread had used 2.36 s of CPU 20 s after boot at 115200, almost
  all of it in `uart_poll_out`;
- at the island's join, `_kernel.usage` did not change for 620 ms (no context
  switch at all) while the RX ring went from 76 to 1024 bytes and latched full;
- the frame lost to that overflow carried `D_KEYEXPR`s, the next `D_TOKEN`
  named one of them, and before issue 1533 that stopped the read task.

At 921600 the same busy-wait is eight times shorter, and a 10 min soak held
with every contracted input at its contract rate -- but only when the board
joined an idle domain and the inputs started after its registration. When the
host was already publishing (28 KB/s toward the board) as it joined, the
registration burst starved the read task again: the board published
`mrm_state` 46 times in its first 4.6 s (trace markers) and the host received
none of them, while `emergency/control_cmd` arrived; about 70 s later the
board's lease expired and it reconnected. That run had no tap on the wire, so
which frames were lost is inferred, not seen: a publisher whose write-filter
interest reply is lost believes it has no remote reader and never sends, which
is what `mrm_state` did. The ordering is still wrong at 921600; it only has a
smaller window.

## What would fix it

1. Give the flush task a stated default on Zephyr, as 0626 did for read and
   lease: in `zpico_open`, when `g_default_flush_task_configured` is false,
   configure it from a knob (`CONFIG_NROS_ZENOH_FLUSH_PRIORITY`, derived below
   the read band so the RX drain preempts the flush). The knob chain for the
   read priority (Kconfig, `nros-zpico-build` ShimConfig, `nros-zephyr-build`,
   the lane-ownership and priority-plan checks) is the template.
2. Interrupt-driven TX for the Zephyr serial link: `uart_irq_tx_enable` and a TX
   ring the ISR drains with `uart_fifo_fill`, the sender blocking on a
   semaphore when the ring is full. The sender then sleeps instead of spinning,
   whatever its priority, and the CPU the busy-wait takes (counted since 1533 in
   `_z_zephyr_serial_stats.tx_busy_cycles`) returns to the executor.

Either alone removes the starvation; (2) also removes the 28-38 ms tick the
island's handler spends transmitting one service request at 115200.

(Measured since, below: (2) alone does NOT remove it, and (1) is only half.)

## Measured 2026-09-29 (safety island phase8-W10)

S32K344 at 921,600 baud, island gateway router, every contracted input at its
contract rate flowing for 20 s BEFORE the board is reset (a late join), then a
second and third reset with the gateway still holding the old session (a
mid-run reset). Board counters over SWD (`_z_zephyr_serial_stats`,
`_z_rx_rejections`); an overflow instrument in the RX ISR recorded the running
thread and the read task's state at each ring overflow.

| image | joins | outputs at the host | RX ring at the join |
| --- | --- | --- | --- |
| this pin (1533 fix in, polled TX) | 3 of 3 fail | `mrm_state` and three more publishers 0/s for the whole run; `emergency/control_cmd` 18.4/s | 602 overflows by 180 ms after boot; 2-5 bad frames |
| + interrupt-driven TX only | 3 of 3 fail | the same four publishers silent | 1115 overflows by 152 ms |
| + main registering at k_thread 5 (below read), polled TX | 3 of 3 pass | all five at rate | 0 overflows, high water 45-824 of 1024 |

The instrument, interrupt-driven TX, main at k_thread 0:

    139 ms running=main                   reader=zpico_read state=0x80 held=1024
    159 ms running=main                   reader=zpico_read state=0x80 held=1024
    188 ms running=<zephyr_thread_wrapper> reader=zpico_read state=0x80 held=1024
    270 ms running=zpico_read             reader=zpico_read state=0x80 held=1024

The read task is READY (0x80) and not running while main registers the
image's entities at Zephyr's default `CONFIG_MAIN_THREAD_PRIORITY` 0, and
while the tx-flush thread runs; the flush thread has no stated priority and
inherits its creator's, main's 0. The busy-wait was never the only way to
hold the CPU: registration is CPU work too. With 1533 fixed the read task no
longer dies on the frames that do arrive, but the lost frames carry the
router's answers to the publishers' write-filter interests, and a
multi-threaded zenoh-pico write filter starts ACTIVE (drop) and opens only on
such an answer (`src/net/filtering.c`), so those publishers never send.

What this issue now carries:

1. The flush task defaults to the READ band on Zephyr (`zpico_open`, when no
   board called `zpico_set_flush_task_config`): equal to the read task, never
   above it.
2. Interrupt-driven TX (zenoh-pico `e28ff603`), with
   `CONFIG_NROS_ZENOH_SERIAL_TX_RING_BYTES` (default 256), and UART framing /
   noise / parity counters beside the overrun count.

Still open: nano-ros does not state the priority the MAIN thread registers
at. On Zephyr it is `CONFIG_MAIN_THREAD_PRIORITY`, default 0, above every
transport band; an image whose peer is already sending must set it below the
read band (the safety island sets 5 against the read task's 4). A derived
default or a priority-plan check that refuses main above the read band would
close this. The RX ring default (1 KiB, 11 ms of line) was also measured at
its edge at a join under load (high water 815-1024, one join in five
overflowed); the island raises `CONFIG_NROS_ZENOH_SERIAL_RX_RING_BYTES` to
2048.
