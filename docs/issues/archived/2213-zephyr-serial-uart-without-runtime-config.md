---
id: 2213
title: "A Zephyr serial link could not open on a UART with no runtime configuration (QEMU's CMSDK UART)"
status: resolved
type: bug
area: [zenoh, zephyr, serial]
severity: medium
found: 2026-11-02
resolved: 2026-11-02
related: [852, 2164]
---

## Measured

A C++ image on QEMU `mps2-an521` (Zephyr 4.4, zenoh-pico at `d7ea8133`) with
the zenoh link `serial/uart@201000#baudrate=115200`, connected through a socat
pty pair to a stock `rmw_zenohd` with a serial listener. The router logged
`Zenoh can be reached at: serial/...`, and the board, about 2.7 s after boot:

    zpico Session -> ConnectionFailed (the two are indistinguishable downstream)
    RMW session open failed — ConnectionFailed

A hexdump of the pty (`socat -x`) showed no byte from the board.

## Cause

Under GDB, `_z_serial_open_from_dev` found the device (`device_get_binding`
returned `__device_dts_ord_41`) and returned `-128` (`_Z_ERR_GENERIC`) from
`uart_configure()`. Zephyr's CMSDK APB UART driver
(`drivers/serial/uart_cmsdk_apb.c`) implements no `configure`, so the call
answers `-ENOSYS`, which Zephyr's UART API documents as "configuration is not
supported by the device". zenoh-pico treated any non-zero result as a failed
open, so the link never sent its first byte and the client's retries gave up.

The same driver backs `mps2/an385`, so `cmake/zephyr/mps2-an385-serial.conf`
images take the same path. Any build with `CONFIG_UART_USE_RUNTIME_CONFIGURE`
off does too.

## Fix

zenoh-pico fork, `nano-ros-1.10.1` at `fed5d711`: `-ENOSYS` from
`uart_configure()` counts as configured (the device keeps its devicetree
settings); every other error still fails the open.

This change first carried its own pin move to `fed5d711`. Before it merged,
phase-521 W6/W6f moved the pin to `c78dbffd`, a descendant that contains
`fed5d711`, so the fix reached main through that move. The same phase made
`NROS_ZENOH_LINK_SERIAL` select `UART_INTERRUPT_DRIVEN` and depend on
`SERIAL_SUPPORT_INTERRUPT`, because `uart_zephyr.c` is an `#error` without
interrupt-driven UART since `26ad8154` retired the polled receive.

## Verified

The same mps2-an521 image with the fix opens the session, and a ROS 2
subscriber (`rmw_zenoh_cpp` on the router) receives the node's
`std_msgs/String` publications: 239 messages in a 30 s run, every state the
node logged, in order (AutoSDV `firmware/nros-arbiter`, `just arbiter
nros-qemu`). The XIAO RP2350's USB CDC-ACM driver implements `configure`, so
that board was never affected.
