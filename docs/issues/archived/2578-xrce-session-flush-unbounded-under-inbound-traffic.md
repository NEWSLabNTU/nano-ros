---
id: 2578
title: "XRCE: `uxr_run_session_time` never returns under steady inbound traffic, so every callback stalls for as long as the traffic lasts"
status: resolved
type: bug
area: [rmw, xrce]
severity: high
found: 2026-09-18
related: [phase-564, rfc-0005, 2576, 2577, 2579]
resolved: 2026-11-15
resolved_in: "phase-564 W2"
---

**Home: [phase-564](../../roadmap/archived/phase-564-xrce-device-session-robustness.md) W2.**

GitHub: NEWSLabNTU/nano-ros#1075 (reported by wicanr2)

**Standalone, one of three XRCE device-side issues from the same bring-up**
(with issue 2577, the reply-token leak, and issue 2576, the Agent restart). No
active phase owns the XRCE backend on the device; phase-519 is the host-side
Agent. Same rig as the serial-only porting notes.

Setup: bare-metal STM32F427 (RTIC) on Renode, XRCE over a serial link,
Micro-XRCE-DDS-Agent v2.4.3, ROS 2 Jazzy. Reported at `fb89182c03`.

## What happens

`uxr_run_session_time(session, 100)` never returns while the node also has a
subscription receiving steady traffic. The executor stops dispatching **all**
callbacks (timers, subscriptions, later service requests) for as long as the
traffic keeps coming (10-34 s in the reporter's runs), and the caller's
service request times out.

Silent from outside: the node is alive, transport counters move, nothing
logs. What identified it was nano-ros's own contract log,
`contract violation: release-jitter-runtime spin measured=<us>`, showing one
`spin_once(10 ms)` over 10 s; PC sampling in the emulator mapped with
`addr2line` pointed at the flush loop.

## Cause

In this Micro-XRCE-DDS-Client version `uxr_run_session_time` loops

```c
while (!timeout) timeout = !listen_message_reliably(session, timeout_ms);
```

so each 100 ms window that receives *anything* extends the loop: no total
bound. A 20 Hz subscription feeds every window. `uxr_run_session_timeout` in
the same client does bound the total time.

Call sites of that shape, re-located on main 2026-11-15 (the report's line
numbers were 211/419/476/661/179):

| file | line | context |
|---|---|---|
| `packages/rmw/xrce/nros-rmw-xrce/src/service.c` | 360 | create service server |
| | 602 | flush after sending a reply |
| | 657 | client sends a request |
| | 889 | create service client |
| `packages/rmw/xrce/nros-rmw-xrce/src/subscriber.c` | 240 | create subscription |

The subscription one bites on every reboot: commands arrive the moment the
subscription exists, so creating the service server after it is delayed (14 s
in one reboot run). `session.c` `xrce_session_drive_io` (now :1287) passes the
remaining time inside an outer total-time loop, but one call there can still
overrun when the inter-arrival gap is shorter than the remaining time: same
class.

## Evidence

42 s phase table, one service call every 3.5 s, 20 Hz command topic
subscribed:

| run | build | stalled | replies |
|---|---|---|---|
| before x3 | pinned commit | 33 s of 42 s | none from the 2nd request on |
| control | same, service removed | 0 s | n/a |
| after x3 | 5 call sites on `uxr_run_session_timeout` | 0 s | 6/6 |

## Suggested fix

Switch the five call sites to `uxr_run_session_timeout` (the reporter runs
this as a local patch and offered a PR), and bound `xrce_session_drive_io`'s
single call too. If `uxr_run_session_time` is meant to be bounded, the bug is
upstream in the client instead.

## Resolution (2026-11-15)

Every session run in the backend goes through one of two bounded helpers in
`internal.h`: `xrce_session_flush` (`uxr_run_session_until_confirm_delivery`)
after buffering, and `xrce_session_pump` (`uxr_run_session_timeout`) where the
backend waits. 17 call sites, including `xrce_session_drive_io` and the graph
probe. The suggested `uxr_run_session_timeout` everywhere was not taken for the
flush sites, because it always waits out its window. `tests/flush_bounded_under_traffic.c`:
one `send_response` under 3 s of traffic took 3101 ms on main and 101 ms fixed.
