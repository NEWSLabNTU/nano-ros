---
id: 2231
title: "The bare-metal mps2-an385 serial talker cannot open a session with ROS's rmw_zenohd"
status: resolved
type: bug
severity: medium
area: [zenoh, serial, embedded, testing]
related: [phase-528, phase-525, phase-521, 2209, 2207]
found: 2026-11-03
resolved: 2026-11-03
resolved_in: "issue 2231 fix: zpico-serial read waits on deadlines"
---

## What

`examples/mps2-an385-baremetal/rust/serial-talker` (locator
`serial/UART_0#baudrate=115200`) boots, prints `Serial ready.`, and ends its
session open in `ConnectionFailed`. No message is ever published. The router
on the other end of the UART is ROS's `rmw_zenohd` (`ros-humble-rmw-zenoh-cpp`
0.1.9, zenoh 1.8.0), with a serial listener. It failed in the test harness's
socat topology (`test_qemu_serial_pubsub_e2e`) and under `nros run`, so the
verb was never the cause.

## Reproduce

`cargo nextest run -p nros-tests --test emulator -E 'test(test_qemu_serial_pubsub_e2e)'`
with the baremetal fixtures built: 0 of 2 runs passed before the fix. Both
images printed `Executor::open failed: Transport(ConnectionFailed)`, the
listener at 1.0 s and the talker at 2.3 s or 10.0 s of guest time.
`NROS_TEST_LOGS=1 ZENOHD_LOG=zenoh_link_serial=trace,zenoh_transport=trace`
shows the router's side, per image:

```
Creating serial link from ".../serial-listener-zenohd"     (the client's INIT, answered)
Accepting link...
Read error on Serial link ...: Unexpected Init flag in message   (127 ms later)
Closing Serial link ...
```

The client sent a second z-serial INIT 127 ms after the router had answered
the first one, the router dropped the link, and the cycle repeated until the
client gave up.

A hand-run socat script (the one the issue first carried) passed on the same
host and the same image. It is a race, and the harness's extra load (two QEMUs
and the test process) is what loses it.

## Root cause

`zpico-serial`'s `_z_read_serial_internal` bounded its waits by COUNTING them:
500 waits of 10 ms for the first byte of a frame ("5 s"), 500 of 1 ms for the
rest of it. phase-521 W5 (22f69d5087) routed those waits through the board's
idle hook (`set_wait_idle`, UART0 RX interrupt plus a SysTick one-shot under
`wfi`), whose contract says it may return early. Under the SDK's QEMU 11 it
always does: `wfi` returns at once (issue 2209). So the 500 waits took a few
milliseconds, the read inside zenoh-pico's `_z_connect_serial` timed out
before the router's INIT+ACK could cross the pty, and `_z_connect_serial`
re-sent INIT to a router that had already accepted the first.

Whether the cell ever passed: W5's commit reports
`test_qemu_serial_pubsub_e2e` passing. That is consistent with this cause
being latent then. Until issue 2207's fix (55a467ea51), `nros-rmw-zenoh`
retried a failed open ten times, and a later attempt could land in a window
the router accepted. 2207 made one attempt the default, which is right, and
left the race nothing to hide behind. This is inferred from the two commits,
not bisected.

## Fix

Both bounds in `_z_read_serial_internal` are deadlines on zenoh-pico's
monotonic clock (`z_clock_now`), and each wait is the
smaller of the remaining time and the old slice. An idle hook that returns
early now costs a few extra polls, never a shortened timeout. Swept for
siblings: `nros-smoltcp`'s waits (`idle_until_ms(start + timeout)`), the
board's XRCE `read` (`clock_ms` deadline) and `zpico_spin_once`'s serial idle
(`z_clock_elapsed_ms`) were already deadline-based. Nothing else counts calls
to a wait hook.

## Verified

- `test_qemu_serial_pubsub_e2e`, solo: 3 of 3 passed (about 10.5 s each), on
  fixtures rebuilt from the fix. `test_qemu_xrce_pubsub_e2e` passed beside it.
- `nros run --expect 'Publishing:' --timeout 60` in
  `examples/mps2-an385-baremetal/rust/serial-talker`: exit 0, matched after
  3.1 s and 3.2 s on two runs.

Not addressed: QEMU 11's `wfi` (issue 2209) still makes an idle serial image
spin a host core. That is a cost, not a correctness problem, once no timeout
depends on the wait's length.
