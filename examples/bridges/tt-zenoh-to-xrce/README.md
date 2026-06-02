# Time-Triggered Bridge: Zenoh → XRCE-DDS (Phase 110.G.bridge)

ARINC-653-style cyclic executive bridging a Zenoh ingress topic
into a Micro-XRCE-DDS Agent under a 10 ms major frame with
non-overlapping ingress / egress slots.

## What it shows

The bridge demonstrates **Phase 110.G time-triggered (TT)
scheduling** wired through the existing multi-RMW bridge pattern
(Phase 104). Two callback handles share one executor; each is
bound to its own auto-Fifo `SchedContext` whose `tt_window_*`
fields gate dispatch to a fixed slot in the major frame.

```
10 ms major frame
┌──────────────────────────────────────────────────────────┐
│ INGRESS         │  idle  │ EGRESS          │  idle       │
│ [0, 3) ms       │        │ [5, 8) ms       │             │
│                 │        │                 │             │
│ zenoh sub fires │        │ 1 kHz drain     │             │
│ ↓ copy bytes    │        │ ↓ publish XRCE  │             │
│ shared buffer ──┘        └─ shared buffer  │             │
└──────────────────────────────────────────────────────────┘
```

The spin_once TT gate (executor/spin.rs:4007+) compares
`now_us % major_frame_us` against each handle's bound
`SchedContext.tt_window_*` and suppresses out-of-slot
dispatch. Under sustained ingress traffic, the egress timer
never publishes during the ingress window and vice versa.

## Build

From the repo root:

```bash
cargo build -p native-rs-bridge-tt-zenoh-to-xrce
```

The example has its own Cargo workspace
(`[workspace]` table in `Cargo.toml`); it does not participate in
the top-level workspace, matching every other example tree.

## Run

Three processes:

```bash
# 1. zenohd
zenohd --listen tcp/127.0.0.1:7447

# 2. Micro-XRCE-DDS Agent
MicroXRCEAgent udp4 -p 8888

# 3. bridge
cargo run -p native-rs-bridge-tt-zenoh-to-xrce
#   Override locators via env:
#   ZENOH_LOCATOR=tcp/192.168.1.10:7447 \
#   XRCE_LOCATOR=192.168.1.10:8888 \
#       cargo run -p native-rs-bridge-tt-zenoh-to-xrce
```

Once running, publish on `/chatter` from any ROS 2 or nano-ros
talker that speaks Zenoh:

```bash
ros2 topic pub /chatter std_msgs/String '{data: "hello"}'
```

The bridge logs every captured message on the ingress side and
every forward to XRCE on the egress side. The XRCE Agent's
verbose mode (`MicroXRCEAgent udp4 -p 8888 -v6`) confirms
DataWriter publishes arriving at the boundary.

## Files

- `src/main.rs` — `apply_time_triggered_schedule` +
  `bind_handle_to_sched_context` wiring; subscription callback +
  egress drain timer; shared single-slot staging buffer.
- `Cargo.toml` — pulls both `nros-rmw-zenoh` and
  `nros-rmw-xrce-cffi` so both backends self-register in one
  binary.

## Determinism note

The bridge bounds the run at 60 s for ergonomic `cargo run`
behavior. For ad-hoc exploration drop the deadline in
`src/main.rs`. For a deterministic timing test, capture
timestamps in both callbacks and assert that every ingress
sample's timestamp falls in `[0, 3) ms` of its major frame and
every egress drain in `[5, 8) ms` — this is the Phase 110.G
acceptance pattern.

## Phase reference

- Phase 110.G — TimeTriggered API (`TimeTriggeredSchedule<N>`,
  `apply_time_triggered_schedule`, per-handle `tt_window_*`
  gates).
- Phase 104.C — multi-RMW bridge pattern (`node_builder.rmw`,
  `extra_sessions`, `with_node_try`).
