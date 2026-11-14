---
id: 2576
title: "XRCE: after an Agent restart the node stays silently dead, with no session-loss signal and no re-open path"
status: open
type: limitation
area: [rmw, xrce, api]
severity: high
found: 2026-09-18
related: [rfc-0005, 2577, 2578, 2579]
---

GitHub: NEWSLabNTU/nano-ros#1084 (reported by wicanr2)

**Standalone, one of three XRCE device-side issues from the same bring-up**
(with issue 2578, the unbounded session flush, and issue 2577, the reply-token leak).
No active phase owns the XRCE backend on the device; phase-519 owns the
host-side Agent but not what the device does when it goes away.

Setup: bare-metal STM32F427 (RTIC) on Renode, XRCE over serial, Agent v2.4.3,
ROS 2 Jazzy, at `ac65af11`.

## What happens

If the Agent restarts after a session is established, the node keeps
spinning, publishes keep failing, no callback fires again, and nothing in the
node's logs says the session is gone. A new Agent on the same link never sees
a `CREATE_SESSION` from the node.

60 s run, commands at 20 Hz, counters sampled every 2 s; Agent killed at
t=20 s, a **new** Agent started at t=28 s on the same PTY (the serial link is
never touched):

| phase | firmware `RX_CMDS` | command-timeout state |
|---|---|---|
| before kill | 0 -> 335 | ok |
| Agent down (8 s) | 391 -> 391 | timed out |
| after new Agent (120 s) | 391 -> 391 | still timed out |

No reboot (boot counter unchanged). Two runs, identical numbers. The new
Agent's log shows nothing past its own startup.

## What a board author can do today

Nothing found:

- `nros-rmw-xrce-cffi` exposes `register()` and the transport-ops setter; no
  ping, no session state, no "session lost" error.
- `spin_once` returns `Ok` while the session is dead.
- Publishes fail, but that is node-level state a board crate owning boot and
  the spin loop cannot see.

The only lever left is "no inbound bytes for N seconds", which is wrong on a
legitimately quiet link (commands stop when the vehicle is idle). For the
reporter an upstream computer restart is a normal event, and today it means
the vehicle controller needs a reboot to talk again.

## Asked

1. Is there an intended pattern for this?
2. If not, a session-liveness signal: `uxr_ping_agent` surfaced through the
   cffi shim, or `spin_once` returning a distinguishable "session lost" error
   so the caller can drop the executor and re-open.

The reporter offered to implement either and has a reproducible rig (kill the
Agent mid-run, start a new one, watch the inbound counter), in the emulator
and on hardware.
