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

## Measured: the board was not RECEIVING at all (2026-08-27)

Counting the board's own debug log over the 0 -> 20.095 s window before the
first expiry:

| direction | events |
| --- | --- |
| `_z_transport_tx_send_n_msg` (transmit) | 26 |
| `_z_keep_alive_encode` (keepalives SENT) | 5 |
| **anything receive-side** | **4 — and all four are handshake decodes** |

So after the handshake the board received nothing. It was not that keepalives
arrived late; none were being processed at all. That rules out "the router is
not sending" as the interesting half — the board had no working receive path.

## Cause: it ran out of thread stack slots, silently

`nros_zephyr_task_create` hands out stacks from a fixed array and returns `-1`
once `nros_thread_index >= NROS_ZEPHYR_MAX_THREADS` (effectively **4** by
default). The action image needs more tasks than the talker — three queryables
and two publishers' worth of machinery — and the task that lost the race is
zenoh-pico's **read** task. The image therefore came up, declared every entity
correctly, transmitted happily, and never received anything.

**The failure was silent**, which is the reason this looked like a keepalive or
link problem for as long as it did. Fixed: exhaustion now prints
`nros: OUT OF THREAD SLOTS (NROS_ZEPHYR_MAX_THREADS=n)` and names the knob.

## Effect of raising the slots

`CONFIG_NROS_ZEPHYR_TASK_SLOTS=8` (with `TASK_STACK_SIZE=6144` to stay inside
RAM):

- receive-side events 4 -> 12
- `ros2 node list` -> `/fibonacci_action_server`
- `ros2 action list` -> `/fibonacci`
- `ros2 action info /fibonacci` -> **`Action servers: 1`** (was 0)

All four action type names already matched a native
`action_tutorials_cpp fibonacci_action_server` byte for byte after
`a4abcccde`/`6d2b67bff`.

## Fixed: the stack-slot LEAK (2026-08-27)

With the loud diagnostic in place, an 8-slot run showed the real shape:

```
63.515  handshake completes
63.539  OUT OF THREAD SLOTS (NROS_ZEPHYR_MAX_THREADS=8) -- task not created
63.539  ERROR ::_zp_unicast_failed] Reopen failed: -79
```

Slots ran out at the **third and fourth reconnect**, not at boot. Raising the
count only moves that wall: `nros_thread_index` only ever ROSE, so every
reconnect spent slots permanently and the image eventually could not reopen at
all.

Replaced with a claim/release table. Release is on **join** and never on
detach, for the same reason as
[issue 0822](archived/0822-zephyr-thread-stack-slots-unbounded.md): a returned
`pthread_join` proves the thread is gone, whereas a detached one may still be
running and handing its stack to the next task would be worse than the leak.

Measured effect, same build, same harness:

| | before | after |
| --- | --- | --- |
| `OUT OF THREAD SLOTS` | 2 | **0** |
| transports opened / closed | 5 / 5 | **1 / 1** |

So the reconnect cascade and the `Reopen failed: -79` dead end are gone.

## STILL OPEN: the first session is starved on receive

This is the actual core of the issue and it is NOT fixed. From the very first
session, before any reconnect or slot pressure:

| | first 25 s |
| --- | --- |
| `_z_transport_tx_send_n_msg` | 26 |
| receive-side events | **2, plus the handshake decodes** |

The board transmits freely and receives essentially nothing, so its lease
expires at `2 x Z_TRANSPORT_LEASE` and it closes with reason 5. A **talker** on
the identical board, router and transport holds its session for a five-minute
soak, so this is specific to the heavier image rather than to the link.

What is now ruled out: slot exhaustion at boot (the diagnostic is silent for
the first session), the router's keepalive cadence (tapped at 10.0 s), the
domain, and the type names.

What is not established: whether the router's keepalives physically reach the
board during that window. The socat tap attempt for this image produced a
zero-byte capture and was not retried — that measurement is still the fork in
the road, and it now has a much smaller haystack: one session, 20 s, no
reconnect noise.

## Previously open, now superseded

`ros2 action send_goal` still fails:

```
Waiting for an action server to become available...
Failed to check if action server is available: rcl node's context is invalid, at ./src/rcl/node.c:428
```

and the session still churns — **5 serial links, 8 opened, 7 closed** in one
run, even with `CONFIG_NROS_ZENOH_LEASE_MS=60000`. So more slots fixed the
*discovery* half but something still drops the session. Whether the remaining
churn is a second starvation (more tasks still needed), a host-side CLI issue,
or genuine loss on a loaded 115200 link is **not established**.

Next: re-run with `NROS_ZENOH_DEBUG=3` at `TASK_SLOTS=8` and read what the
board says at each close — the expiry message names its own lease, so it
distinguishes "expired again" from "closed for another reason" immediately.

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
