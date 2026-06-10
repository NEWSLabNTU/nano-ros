---
id: 26
title: nros-rmw-xrce — pub+sub node free-ran its spin loop and closed before DDS discovery (poll-based pacing)
status: resolved
type: bug
area: rmw-xrce
related: [phase-233, rfc-0039]
---

A nano-ros XRCE node holding **both a publisher and a subscriber in one
session** failed to receive on the subscriber, breaking the PX4 companion
(`examples/px4/rust/xrce/offboard-companion`). **Root cause: spin pacing.
Fixed.**

> **Resolution (Phase 233.4).** Deep debugging — tshark packet capture, a
> logging-enabled agent rebuild (`-DUAGENT_LOGGER_PROFILE=ON`), and gdb —
> traced it to one bug. (Two earlier diagnoses were wrong: "bare agent can't
> match `px4_msgs` types" — message type is irrelevant; and a supposed
> "mixed-direction reader+writer never matches at the agent" — that was an
> artifact of a polluted ad-hoc test environment reusing `ROS_DOMAIN_ID=0`
> across hundreds of runs. With unique domains the companion receives **5/5**.)
>
> **Root cause — poll-based spin pacing.** XRCE is a *poll-based* backend (no
> wake callback, unlike Zenoh): the executor's `spin_once(t)` paces by relying
> on the backend to block for `t`. But `uxr_run_session_time` returns the
> instant the reliable output streams are confirmed, so a session **with a
> publisher** (unconfirmed WRITE_DATA) returned in ~0 µs instead of `t` ms
> (measured: 0 µs with a publisher vs 11 ms without). The spin loop then
> free-ran — a pub+sub node burned through a bounded loop in ~1 ms and sent
> `DELETE_CLIENT` (close) before DDS discovery completed, so the agent tore the
> subscriber's datareader down (proven on the wire: a `DELETE` submessage with
> `object_id=0xFFFE` as the node's *last* packet at t≈0.12 s; agent
> `~DataReader` + `delete_client` immediately after).
>
> **Fix.** `xrce_session_drive_io` now drives the session across the whole
> `t` ms window — each pass services inbound (delivering subscriber samples) —
> and yields ~1 ms when a pass returns early, so `spin_once(t)` consumes ~`t` ms
> wall-clock as the executor expects, without busy-spinning. Mirrors the
> `zpico_spin_once` `z_sleep_ms` fix for multi-threaded platforms.
>
> **Validated:** `nros-tests::px4_xrce::{test_px4_msgs_roundtrip_over_agent,
> test_px4_companion_cross_session_receive}` pass; the companion cross-session
> test is a regression guard. `nros-tests::xrce::{talker_listener_communication,
> multiple_messages}` still pass — no regression.

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

## Mechanism (agent-confirmed)

Rebuilt the bundled `MicroXRCEAgent` with logging on
(`cmake build/xrce-agent -DUAGENT_LOGGER_PROFILE=ON && cmake --build …`; the
shipped binary is built `-DUAGENT_LOGGER_PROFILE=OFF`, hence the earlier
log-silence) and ran `MicroXRCEAgent udp4 -p <port> -v 6`.

The decisive signal is `DataReader.cpp read_fn` — the agent reading a sample
from DDS and forwarding it to the subscribing client:

- **sub-only (works):** `read_fn` fires ~95×; it starts the moment the matched
  external writer comes online (data-driven).
- **dual (broken):** `read_fn` fires **0×**. The agent's DataReader never reads
  → never forwards → the client's `xrce_topic_callback` never fires (also
  confirmed 0× with an `fprintf` probe). Entity creation all succeeds
  (`create_participant`/`create_datareader`/`create_datawriter` all logged OK,
  no errors/NACKs); the agent's DataWriter `write` fires for the publisher
  traffic. Only the *reader* side is dead.

So the agent stops delivering to a client's datareader **whenever that same
client/session also holds a datawriter** — the DDS reader↔writer match for that
client's reader never produces `on_data_available`. Deterministic (0/8).

Agent internals: each datareader gets a dedicated read thread
(`include/uxr/agent/reader/Reader.hpp:102` → `read_task` → `read_fn`) that
`take()`s from its Fast-DDS DataReader. `read_fn` = 0 means that Fast-DDS
DataReader never received — i.e. it never matched the external writer — so the
failure is at the **Fast-DDS matching** layer for the mixed-direction client's
reader, not in `nros-rmw-xrce`. (Confirming whether it is Fast-DDS intra-process
discovery vs the agent's ProxyClient endpoint setup needs Fast-DDS-level logs —
`export FASTDDS_…` — a further layer down.)

`nros-rmw-xrce` runs **one** reliable output stream + one reliable input stream
per session (`session.c:381` `uxr_create_output_reliable_stream` /
`uxr_create_input_reliable_stream`). Both the subscriber's `request_data` and
every publisher `uxr_buffer_topic` WRITE_DATA share that single
`output_reliable` stream (`subscriber.c:170`, `publisher.c:122`); inbound
samples arrive on `input_reliable`.

## What was ruled out (custom 2-process harness, RELIABLE QoS, 8 runs each)

The condition is the **datawriter in the reader's session**, not any of:

| attempted change                                              | dual hits |
|---------------------------------------------------------------|-----------|
| baseline                                                      | 0 / 8     |
| drop the publish-time `run_session(0)` flush                  | 1 / 8     |
| publisher WRITE_DATA on a separate **best-effort** out stream | 0 / 8     |
| best-effort stream **+** no publish-time flush                | 2 / 8     |
| publisher in a **separate DDS participant** (id 2)            | 0 / 8     |
| publish at 10 Hz instead of every spin                        | 0 / 8     |
| (sub-only, reference)                                         | ~4 / 8    |

None restore sub-only levels; the participant split (homogeneous reader-only
participant) and the publish-rate cut both still 0/8 — so it is **session/
client-level**, inside the micro-XRCE-DDS agent or client, not a stream / QoS /
participant / rate problem in `nros-rmw-xrce`. (The 1–2/8 from dropping the
flush is in the discovery-race noise band, not a real fix.) All experimental C
patches were reverted.

The proven workaround is **separate XRCE sessions**: running the publisher and
subscriber as two processes (two sessions, two agent connections) recovers the
sub-only ~50% — i.e. a reader-only session matches.

## Fix directions (not yet done)

1. **Separate XRCE sessions per direction** — give publishers their own session
   (agent connection) so the subscriber session stays reader-only. Proven to
   work, but a real architecture change: the executor opens one
   `ConcreteSession`; this needs the backend to manage two (sub + pub) and
   route entity creation accordingly.
2. **Fix the agent/client mixed-direction bug** — root the agent-side reason a
   client's datareader stops matching once the client gains a datawriter
   (vendored Micro-XRCE-DDS-Agent / -Client). Repro: the logging-enabled agent
   above shows `read_fn` = 0 for the dual client. Benefits all mixed pub+sub
   XRCE nodes, not just PX4.
3. **BEST_EFFORT discovery race** (separate defect) — even a reader-only session
   matches only ~50% with a 2 s sub→pub gap; needs a longer discovery window or
   reliable-on-discovery.

Until fixed, the companion example streams setpoints (`/fmu/in/*`, publish
works) but does not reliably receive `/fmu/out/*`. The single-session loopback
CI test (`px4_xrce`) stays valid for the serialization/QoS surface (its pub+sub
are the *same* session+topic, which matches intra-participant and sidesteps the
mixed-direction match failure).
