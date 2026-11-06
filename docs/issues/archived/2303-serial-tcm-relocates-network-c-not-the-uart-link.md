---
id: 2303
title: "`CONFIG_NROS_ZENOH_SERIAL_TCM` relocates `system/zephyr/network.c`, but the serial link is `link/transport/serial/uart_zephyr.c` now, so nothing of the link moves"
status: resolved
type: bug
area: [zephyr, rmw]
severity: high
found: 2026-11-06
resolved: 2026-11-06
related: [phase-474]
---

## What

phase-474 I13 added `CONFIG_NROS_ZENOH_SERIAL_TCM` to put the serial link's
code in ITCM and its zero-initialised data (the RX and TX rings, the framing
buffers, the UART_ASYNC DMA buffers) in DTCM. `zephyr/cmake/nros_rmw_zenoh.cmake`
did that by relocating zenoh-pico `src/system/zephyr/network.c`, where the link
lived then. zenoh-pico has since moved the link to
`src/link/transport/serial/uart_zephyr.c`; `network.c` keeps only the socket
backend. With the option on, the configure found `network.c` and relocated it,
so the build passed and the link stayed where it was.

`CONFIG_NROS_ZENOH_SERIAL_UART_ASYNC` depends on SERIAL_TCM on a part with a
D-cache because the eDMA must not write cached SRAM. With the old relocation
that guarantee was void: on the safety island (S32K344, nano-ros 984c614585)
`_z_serial_async_rx_buf`, `_z_serial_rx_storage` and `_z_serial_tx_storage`
sat at 0x2042xxxx (cached SRAM) with SERIAL_TCM on. The island worked around
it by relocating `uart_zephyr.c` itself.

## Fix

The configure now relocates `link/transport/serial/uart_zephyr.c` (ITCM_TEXT
and DTCM_BSS) and stops with an error naming that file when it is not among
the zenoh-pico sources. The Kconfig help of SERIAL_TCM, the two ring sizes,
UART_ASYNC and ISR_STATS names the new file.

Check on an image: `nm` shows the three buffers above at 0x2000xxxx/0x2001xxxx
(DTCM on the S32K344), not 0x204xxxxx.
