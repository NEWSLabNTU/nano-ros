---
id: 26
title: nros-rmw-xrce — a session with both a publisher and a subscriber starves the subscriber's receive
status: open
type: bug
area: rmw-xrce
related: [phase-233, rfc-0039]
---

An nano-ros XRCE node that holds **both a publisher and a subscriber in one
session** receives few or no samples on the subscriber. This breaks the PX4
companion (`examples/px4/rust/xrce/offboard-companion`, which subscribes
`/fmu/out/vehicle_odometry` *and* publishes `/fmu/in/offboard_control_mode`) —
**including against a real PX4 agent**, not just a bare one.

> **History.** This issue first read as "bare agent can't match `px4_msgs`
> types cross-session." That diagnosis was **wrong** — an artifact of (a) a
> flaky BEST_EFFORT discovery race and (b) the real pub+sub bug below. A
> non-built-in custom type *does* match cross-session on a bare agent; the
> message type is irrelevant.

## Minimal repro

Two host processes, one bare `MicroXRCEAgent udp4`, BEST_EFFORT (`px4()`) QoS,
subscriber up first, then a publisher streams `VehicleOdometry` on
`/fmu/out/vehicle_odometry`. Vary only the *subscriber* process's entities;
5 runs each (rx = samples the subscriber received):

| subscriber-process entities                       | hits (rx>0) |
|---------------------------------------------------|-------------|
| sub only                                          | 2–3 / 5     |
| sub + a publisher (on `/fmu/in/...`), never used  | 1 / 5       |
| sub + a publisher actively publishing             | **0 / 5**   |

The companion is the last row → never receives. Entity-creation order
(pub-before-sub vs sub-before-pub) does not matter. Reproduced with both a
hand-written custom type and the generated `px4_msgs` types, on `/chatter`,
`/xsess_topic`, and `/fmu/out/*`.

Two distinct defects show up:

1. **Pub+sub starvation (primary).** Adding a publisher to a subscriber's
   session degrades receive; *active* publishing kills it. Companion-shaped.
2. **BEST_EFFORT discovery flakiness (secondary).** Even a *sub-only* session
   only matches ~50% of runs under BEST_EFFORT with a 2 s sub→pub gap — a
   discovery/timing race, not deterministic.

## What works (controls)

- **Single-session loopback** — pub + sub on the **same** topic in one session
  round-trips reliably (`px4-stub` `PX4_STUB_LOOPBACK=1`, rx≈117/120). The
  writer feeds the reader intra-participant, so it never exercises the broken
  external-receive path. This is what `nros-tests::px4_xrce` covers — it
  validates `px4_msgs` CDR + `px4()` QoS + the XRCE pub/sub path, but **not**
  the pub+sub cross-session receive.
- **Cross-session, sub-only, non-built-in custom type** — matches on a bare
  agent (flakily, per defect 2). Disproves the old "type/typed-agent" theory.

## Likely cause

`nros-rmw-xrce` runs **one** reliable output stream + one reliable input
stream per session (`session.c` `uxr_create_output_reliable_stream` /
`uxr_create_input_reliable_stream`). Both the subscriber's `request_data` and
every publisher `uxr_buffer_topic` WRITE_DATA share that single
`output_reliable` stream (`subscriber.c:170`, `publisher.c:122`); inbound
samples arrive on `input_reliable`. Publisher traffic on the shared reliable
output stream appears to interfere with the continuous data-delivery /
ACK flow the subscriber needs — the agent stalls delivery on `input_reliable`.
`publish_raw` flushes with `uxr_run_session_time(session, 0)` (zero timeout),
which may not service inbound/ACK adequately when interleaved with the spin
loop. (Echoes the known `request_data`-flush hazard already fixed for the
service path — see the zenoh/XRCE notes in agent memory.)

The agent ships with logging compiled out, so agent-side matched/delivery
state could not be observed directly; the above is inferred from the client
stream model + the behaviour table.

## Fix directions (not yet done)

1. **Separate publisher data onto a best-effort output stream** for BEST_EFFORT
   QoS (don't share the reliable output stream the subscriber's control +
   delivery depend on). Most likely the right fix and QoS-correct.
2. **Service inbound on every publish** — give `publish_raw`'s
   `uxr_run_session_time` a small nonzero budget, or run the input stream
   after each publish so delivery/ACK keeps flowing.
3. **Investigate the BEST_EFFORT discovery race** (defect 2) separately — may
   need a longer discovery window or a reliable-on-discovery handshake.

Until fixed, the companion example streams setpoints (`/fmu/in/*`, publish
works) but does not reliably receive `/fmu/out/*`. The single-session loopback
CI test (`px4_xrce`) stays valid for the serialization/QoS surface.
