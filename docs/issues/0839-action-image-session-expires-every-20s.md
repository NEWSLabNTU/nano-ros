---
id: 839
title: "The action-server image's zenoh session expires every 20 s under a router
  that keeps a talker session alive for minutes"
status: open
type: bug
area: rmw
related: [issue-0821, issue-0824]
---

## Problem

`examples/zephyr/c/action-server` on mr_canhubk3/s32k344, zenoh over serial,
against the router config this repo ships
(`experiments/serial-interop/router-serial.json5`, `keep_alive: 6` = one
keepalive every 10 s). The board declares every entity correctly and then loses
the session on a 20 s cycle:

```
[20.095000 INFO ::_zp_unicast_lease_task] Closing session because it has expired after 10000ms
[41.612000 INFO ::_zp_unicast_lease_task] Closing session because it has expired after 10000ms
```

Router side over one run: **3 serial links, 4 transports opened, 3 closed.**
`ros2 node list`, `ros2 action list` and `ros2 topic list` all stay empty —
discovery never converges inside a 20 s window.

## Why this is not [issue 0821](archived/0821-zenoh-pico-faults-at-lease-expiry-on-zephyr.md)

0821 was the board *faulting* at expiry; that is fixed and the board here is
healthy (`0` faults across the run, it reconnects cleanly). This is the expiry
itself still happening when it should not.

## Why this is not the router config

The identical router config, same board, same transport, holds a **talker**
session for a 5-minute soak with `closed=0` and `ros2 topic hz` steady at
1.99 Hz. The keepalive cadence was verified on the wire with a socat tap at
10.0 s intervals. So the router is speaking; this image is not hearing it in
time.

## Suspicion, not yet established

The action image is much heavier than the talker: **3 queryables**
(send_goal / cancel_goal / get_result) **+ 2 publishers** (feedback, status)
plus their callbacks, on a 115200 baud link. `_received` is set by the read
task, so if that task is starved — or is busy decoding a burst — the lease task
can hit its deadline with inbound keepalives sitting unprocessed.

A framing complaint appears in the same log and may be related or may be a
symptom of the reconnect:

```
[41.615000 DEBUG ::_z_serial_msg_deserialize] decoded frame too small
```

## Suggested next measurements

- Tap the line (socat PTY, as in 0821) and confirm the router's keepalives are
  physically arriving during the 20 s before expiry. That separates "not sent"
  from "sent but not processed" and is the fork in the road.
- If they arrive: instrument `_received` / the read task to see whether the
  frames are being consumed late.
- Raise `CONFIG_NROS_ZENOH_LEASE_MS` as a *diagnostic* only — if the expiry
  simply moves proportionally, it confirms starvation rather than loss.

## Impact

Actions cannot be exercised end to end over serial. The type names and domains
on every action entity are correct as of `a4abcccde`, so this is the only thing
between the action example and a working `ros2 action send_goal`.
