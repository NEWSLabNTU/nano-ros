---
id: 2576
title: "XRCE: after an Agent restart the node stays silently dead, with no session-loss signal and no re-open path"
status: resolved
type: limitation
area: [rmw, xrce, api]
severity: high
found: 2026-09-18
related: [phase-564, rfc-0005, 2577, 2578, 2579]
resolved: 2026-11-15
resolved_in: "phase-564 W4"
---

**Home: [phase-564](../../roadmap/archived/phase-564-xrce-device-session-robustness.md) W4.**

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

## Measured on main (19612e8657)

A host rig over UDP instead of the reporter's serial PTY, same shape. One
session publishes at 10 Hz through a proxy that logs every XRCE submessage;
Agent A is killed at 6 s and a fresh Agent B (v2.4.3) starts at 9 s.

- `drive_io`, so `spin_once`, answered OK throughout. Publishes answered OK
  until the reliable window filled (~1.5 s), then MESSAGE_TOO_LARGE, a wrong
  code for a full window.
- Agent B received no CREATE_CLIENT and no WRITE_DATA in the 12 s after it
  started: the report, reproduced.
- The one exchange left on the wire was the session ping, and Agent B
  ANSWERED every GET_INFO with INFO "available, no session". The client
  decodes that (`PONG_NO_SESSION_STATUS`) and `uxr_ping_agent_session`
  returns false for it and for silence alike, so `Executor::ping` said
  TIMEOUT while a live Agent was saying the session was gone. The signal
  existed and was thrown away.

To the reporter's first question: `Executor::ping` did exist, but it could not
tell a dead Agent from a restarted one, and nothing drove it.

## Rulings (user, 2026-11-15)

1. The signal, all three parts:
   - `ping_session` answers a third way, a new `NROS_RMW_RET_SESSION_LOST`
     (Rust `TransportError::SessionLost`) for "the Agent answered and holds no
     session"; TIMEOUT stays "no answer".
   - Passive detection on a quiet link: after `XRCE_SESSION_PROBE_MS` of
     inbound silence `drive_io` sends one GET_INFO itself, and only the
     Agent's "no session" answer latches the loss. No byte-count heuristic,
     which the reporter rightly objected to for an idle vehicle.
   - A write refused for a full window is WOULD_BLOCK (SESSION_LOST once
     latched), not MESSAGE_TOO_LARGE.
2. Re-open by the application: drop the executor and open again, over the same
   caller-supplied backing (`open_in`), INCLUDING the `alloc` `open()` family
   giving its default backing back on drop. Transparent recovery inside the
   backend (re-create the session and replay every entity) is deferred.
3. `XRCE_SESSION_PROBE_MS` defaults to 1000 ms; 0 turns the probe off.

## Resolution (2026-11-15)

- `session.c` taps the session's receive (`last_rx_ms`), and
  `xrce_liveness_tick` runs after every session run in `drive_io`: it latches
  `session_lost` on a "no session" answer and probes a link silent for
  `XRCE_SESSION_PROBE_MS` (a compile-time define in `internal.h`, like the
  backend's other timeouts). Once latched, `drive_io`, `publish` and
  `ping_session` return SESSION_LOST, the destroy skips the DELETE_CLIENT an
  Agent with no session cannot answer, and one error line says to re-open.
- `xrce_output_refusal` names a publish refusal: SESSION_LOST, WOULD_BLOCK for
  a full window, or MESSAGE_TOO_LARGE.
- The executor keeps the verdict apart from its I/O failure tally:
  `Executor::session_lost()`.
- `with_default_backing` hands the backing to the executor's last field,
  `OwnedBacking`, which gives the static back (or frees the heap block) on
  drop, so a re-open through `Executor::open` costs no new allocation.

Measured with the same rig on the fixed tree:

| tree | Agent B sees CREATE_CLIENT | WRITE_DATA to B | signal |
| --- | --- | --- | --- |
| main | never | 0 | none: `spin_once` OK, ping TIMEOUT |
| fixed, backend rig (drop + create the session) | 1.1 s after B started | 113 in 10 s | `drive_io` SESSION_LOST about 1 s after B started |
| fixed, `Executor` rig (`session_lost()`, drop, `Executor::open`) | about 1 s after B started | 159 in 11 s | `session_lost()` true 0.9 s after B started |

While the Agent was down, ping answered TIMEOUT and nothing latched, so an
Agent that comes back with the session intact carries on. Teardown of the lost
session took 1 ms.

Tests: `tests/session_lost.c` (no Agent: a fake one replays Agent v2.4.3's
INFO bytes in and out of session) covers the three ping answers, the latch,
the quiet-link probe, a link that stays in session, a busy link going
unprobed, and the full-window and too-large refusals.
`executor::backing::tests::the_static_comes_back_after_a_failed_open_or_a_drop`
covers the drop give-back for the static and heap arms.
