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

## Mechanism (instrumented)

The subscriber's topic callback (`xrce_topic_callback`, `subscriber.c`) **never
fires** in the dual case — confirmed with an `fprintf` at callback entry: 0
invocations across the run. So the agent stops *delivering* samples to the
datareader; it is not a client-side drop (slot mismatch / ring-full / locked).

`nros-rmw-xrce` runs **one** reliable output stream + one reliable input stream
per session (`session.c:381` `uxr_create_output_reliable_stream` /
`uxr_create_input_reliable_stream`). Both the subscriber's `request_data` and
every publisher `uxr_buffer_topic` WRITE_DATA share that single
`output_reliable` stream (`subscriber.c:170`, `publisher.c:122`); inbound
samples arrive on `input_reliable`.

Isolation experiments (custom 2-process harness, RELIABLE QoS, 6–8 runs each;
the agent ships with logging compiled out, so this is client-side bisection):

| change to the publisher path                                  | dual hits |
|---------------------------------------------------------------|-----------|
| baseline (`publish_raw` → `output_reliable` + `run_session(0)`)| **0 / 8** |
| drop the publish-time `run_session(0)` flush                   | 1 / 8     |
| route WRITE_DATA to a separate **best-effort** output stream   | 0 / 1     |
| best-effort stream **and** no publish-time flush              | 2 / 8     |
| (sub-only, for reference)                                      | ~4 / 8    |

Reading: the publish-time `uxr_run_session_time(&session, 0)` flush
(`publisher.c:128`) is the **primary** contributor — calling it per-publish,
interleaved with the spin's own `run_session`, disrupts the reliable
input-stream delivery/ACK cycle so the agent stalls delivery. Moving publisher
data off the reliable output stream helps a little but does **not** restore
sub-only levels, so there is residual interference from the publisher's
session activity beyond stream choice — the exact reliable-stream state
interaction is unresolved and needs the micro-XRCE-DDS client reliable-stream
machine stepped through (or the agent rebuilt with logging).

## Fix directions (not yet done — experimental patches reverted)

1. **Don't flush per-publish** — let the executor's spin `run_session` push
   buffered WRITE_DATA instead of `publish_raw` calling `run_session(0)` every
   call. Biggest single lever (0/8 → 1/8). Needs a guard so a publish-then-exit
   (no spin) path still flushes.
2. **Separate publisher data onto a best-effort output stream** for BEST_EFFORT
   QoS (QoS-correct anyway). Helps in combination with (1) (→ 2/8) but is not
   sufficient alone.
3. **Find the residual interference** — even fully decoupled the publisher
   still degrades receive vs sub-only. Likely the shared single reliable
   session state machine; instrument the client (`uxr_run_session*`,
   reliable-stream seq/ACK) or rebuild MicroXRCEAgent with logging to see why
   the agent stops delivering on `input_reliable`.
4. **BEST_EFFORT discovery race** (defect 2) — separate ~50% flakiness even
   sub-only; needs a longer discovery window or reliable-on-discovery.

Until fixed, the companion example streams setpoints (`/fmu/in/*`, publish
works) but does not reliably receive `/fmu/out/*`. The single-session loopback
CI test (`px4_xrce`) stays valid for the serialization/QoS surface.
