# PX4 XRCE-DDS companion bring-up

How to stand up a nano-ros node as a **companion / peer** of PX4 over the
Micro XRCE-DDS bridge — the mainstream PX4 ↔ ROS 2 integration. This is
**Track B** of RFC-0039 (Phase 233); the additive path that reuses
`nros-rmw-xrce` as an XRCE-DDS client beside PX4's own `uxrce_dds_client`.

For the in-firmware uORB path (Track A) see RFC-0039 + Phase 232.

## Picture

```
                       /fmu/out/*  (PX4 → peers)
PX4 (uxrce_dds_client) ───────────► MicroXRCEAgent ──DDS──► ROS 2 nodes
        ▲                              ▲      │
        │ /fmu/in/* (peers → PX4)      │      └──────────► nano-ros companion
        └──────────────────────────────┘                  (nros-rmw-xrce)
```

PX4 publishes telemetry on `/fmu/out/*` and consumes commands on `/fmu/in/*`.
A nano-ros node connects to the **same** agent and exchanges `px4_msgs`. The
agent bridges every XRCE client into one DDS graph, so the companion, PX4, and
any ROS 2 node interoperate.

## 1. Generate `px4_msgs`

CDR `px4_msgs::msg::*` types come straight from the PX4 `.msg` tree — no ament
`px4_msgs` dependency:

```bash
nros generate-px4-msgs \
  --px4 third-party/px4/PX4-Autopilot \
  --output generated
# → generated/px4_msgs (a path-dep crate)
```

The example crates under `examples/px4/rust/xrce/` carry a trimmed,
pre-generated `generated/px4_msgs/` (just the topics they touch).

## 2. QoS — must match PX4

PX4's endpoints are **`BEST_EFFORT` + `TRANSIENT_LOCAL` + `KEEP_LAST(1)`**.
The default reliable+volatile profile will **not** match them. Use the named
profile:

```rust
use nros::prelude::*;
let qos = QosSettings::px4();          // BEST_EFFORT + TRANSIENT_LOCAL + KEEP_LAST(1)
// raise depth for high-rate streams: QosSettings::px4().keep_last(10)
```

`nros-rmw-xrce` already lowers both policies (`xrce_map_qos`, `session.c`).

## 3. Start the agent

PX4's default companion link is UDP on port 8888:

```bash
MicroXRCEAgent udp4 -p 8888
```

Serial / other transports: see the PX4 uXRCE-DDS docs. `nros setup` provisions
a `MicroXRCEAgent` under `~/.nros/bin/`; the in-tree build is
`build/xrce-agent/MicroXRCEAgent`.

> Known limitation: the companion holds a subscriber and a publisher in one
> session, and `nros-rmw-xrce` currently starves the subscriber's receive in
> that shape — so the companion streams setpoints but rarely receives
> `/fmu/out/*`. Tracked in
> [issue 0026](../issues/0026-px4-xrce-bare-agent-type-matching.md).

## 4. Run PX4

SITL:

```bash
cd PX4-Autopilot
make px4_sitl gz_x500          # or jmavsim / none_iris
# uxrce_dds_client autostarts and connects to the agent on 8888
```

Real hardware: enable `MAV_1_CONFIG` / `UXRCE_DDS_CFG` on a serial/UDP port and
point the agent at it.

## 5. Run the companion

```bash
NROS_LOCATOR=127.0.0.1:8888 cargo run -p px4-offboard-companion
```

It subscribes `/fmu/out/vehicle_odometry` and streams
`/fmu/in/offboard_control_mode` at ~10 Hz with the PX4 QoS profile. Source:
`examples/px4/rust/xrce/offboard-companion/`.

## Driving it without PX4 SITL

`examples/px4/rust/xrce/px4-stub` plays the PX4 side — it publishes
`VehicleOdometry` on `/fmu/out/vehicle_odometry`. Point both at the same agent
to exercise the companion's publish/connect path:

```bash
MicroXRCEAgent udp4 -p 8888 &
NROS_LOCATOR=127.0.0.1:8888 PX4_COMPANION_TICKS=200 cargo run -p px4-offboard-companion &
NROS_LOCATOR=127.0.0.1:8888 PX4_STUB_TICKS=120        cargo run -p px4-stub
```

The companion **receive** is currently unreliable regardless of agent: a
session with both a publisher and a subscriber starves the subscriber in
`nros-rmw-xrce` — [issue 0026](../issues/0026-px4-xrce-bare-agent-type-matching.md).
The single-session loopback (`px4-stub PX4_STUB_LOOPBACK=1`) round-trips fine
and is what CI exercises.

## Topics

| Direction        | Topic                              | Type                            |
|------------------|------------------------------------|---------------------------------|
| PX4 → companion  | `/fmu/out/vehicle_odometry`        | `px4_msgs/VehicleOdometry`      |
| companion → PX4  | `/fmu/in/offboard_control_mode`    | `px4_msgs/OffboardControlMode`  |

More `/fmu/{in,out}/*` topics: PX4's `dds_topics.yaml`.
