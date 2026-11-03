---
id: 2231
title: "The bare-metal mps2-an385 serial talker cannot open a session with ROS's rmw_zenohd"
status: open
type: bug
severity: medium
area: [zenoh, serial, embedded, testing]
related: [phase-528, phase-525]
found: 2026-11-03
---

## What

`examples/mps2-an385-baremetal/rust/serial-talker` (locator
`serial/UART_0#baudrate=115200`) boots, prints `Serial ready.`, and ends its
session open in `ConnectionFailed` about 10 s later. No message is ever
published. The router on the other end of the UART is ROS's `rmw_zenohd`
(`ros-humble-rmw-zenoh-cpp`, the RFC-0075 router), with a serial listener.

It fails in two topologies that share nothing but the image and the router:

1. **The test harness's.** A socat pty pair, `rmw_zenohd` listening on one
   end (`listen/endpoints=["serial/<pty>#baudrate=115200"]`), and QEMU on
   the other (`-chardev serial,path=<pty> -serial chardev:ser0`), with the
   router started 2 s before QEMU. This is the shape of
   `test_qemu_serial_pubsub_e2e`.
2. **`nros run`'s.** QEMU's own `-serial pty`, started paused. The qemu
   runner reports the pty, `nros run` starts `rmw_zenohd` listening on it,
   and the guest resumes 2 s later.

In both, the router logs only `Read error on Serial link ... early eof` once
QEMU exits. When the guest is not held back (the router opens the pty after
the guest has booted), the router instead logs
`Unexpected Init flag in message` twice, so bytes do arrive and frame
correctly. The session handshake is what fails.

## Reproduce

`nros run --expect 'Publishing:' --timeout 60` in the example directory, or
the socat script below (paths shortened):

```sh
socat pty,raw,echo=0,link=qemu.pty pty,raw,echo=0,link=router.pty &
ZENOH_CONFIG_OVERRIDE='scouting/multicast/enabled=false;listen/endpoints=["serial/router.pty#baudrate=115200"]' \
    ros2 run rmw_zenoh_cpp rmw_zenohd &
sleep 2
qemu-system-arm -cpu cortex-m3 -machine mps2-an385 -display none -monitor none \
    -icount shift=auto -semihosting-config enable=on,target=native \
    -chardev serial,id=ser0,path=qemu.pty -serial chardev:ser0 -kernel qemu-serial-talker
```

QEMU is the SDK store's `qemu/11.0.0-nros7`.

## Not yet known

Whether this is the zenoh-pico 1.10 move (phase-508), the phase-521 W6
receive-path changes on bare metal, or the router's zenoh version. The
network talker on the same board, and the network and Zephyr `native_sim`
talkers, open sessions with the same router.

The Zephyr CMSDK UART `configure` failure (fixed by a zenoh-pico bump in
flight) is a different board support path: that image never sent a byte,
and this one does.

## Blocks

- phase-528 W4: the serial-talker acceptance run (`nros run --expect` on
  `examples/mps2-an385-baremetal/rust/serial-talker`). The verb's side of
  the link works: the qemu runner reports the pty and `rmw_zenohd` listens
  on it.
- phase-525 W4: the serial-first first chapter on QEMU, whose image dials
  the router over this same UART-to-pty link.
