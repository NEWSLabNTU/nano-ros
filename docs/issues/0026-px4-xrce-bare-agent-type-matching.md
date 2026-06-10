---
id: 26
title: px4_msgs round-trip needs a typed agent — bare MicroXRCEAgent matches only built-in ROS types
status: open
type: limitation
area: rmw-xrce
related: [phase-233, rfc-0039]
---

The PX4 XRCE companion path (Phase 233 / RFC-0039 Track B) works against the
agent **PX4 runs** but not against a *bare* `MicroXRCEAgent` started with no
type configuration. This blocks a fully self-contained CI round-trip for the
`examples/px4/rust/xrce/offboard-companion` example.

## Symptom

Two nano-ros XRCE clients on one agent (`examples/px4/rust/xrce/px4-stub`
publishes `px4_msgs/VehicleOdometry` on `/fmu/out/vehicle_odometry`; the
companion subscribes it):

```
MicroXRCEAgent udp4 -p 8888           # bare agent, no -r refs
px4-stub        → publishes 80 samples, all OK
offboard-companion → rx = 0           # never matched, nothing received
```

The same two-binary / one-agent pattern with `std_msgs/Int32` on `/chatter`
(`examples/native/rust/{talker,listener}` built `--features rmw-xrce`) works:
the listener receives every sample. Controlled A/B — identical agent, harness,
ordering (subscriber up first), domain, QoS — the **only** difference is the
message type. `std_msgs::msg::dds_::Int32_` matches; `px4_msgs::msg::dds_::*_`
does not. It is not size-related: the 15-byte `OffboardControlMode` fails the
same way as the 108-byte `VehicleOdometry`. It reproduces under `-m dds`,
`-m rtps`, and `-m ced`.

## Cause

`nros-rmw-xrce` creates entities by **binary** (`uxr_buffer_create_topic_bin`
+ `create_datawriter_bin` / `create_datareader_bin`, `subscriber.c` /
`publisher.c`) carrying only the DDS topic name + type **name** — no
`TypeObject` / IDL. A bare agent can therefore only match endpoints for types
its own DDS plugin already knows; the bundled build resolves built-in ROS
types but not `px4_msgs`. A real PX4 deployment runs the agent **with px4_msgs
typesupport** (the agent is built/run alongside `px4_msgs`), so PX4's
`uxrce_dds_client` and an nano-ros companion match there.

## Impact

- The `offboard-companion` example is correct for a real PX4 agent; the
  publish (`/fmu/in/*`) and subscribe (`/fmu/out/*`) wiring + the `px4()` QoS
  profile are exercised, and the session/entity creation succeeds.
- A self-contained CI round-trip (bare stub agent) cannot assert *receive*.
  The CI-able surface against a bare agent is the connect + publish path.

## Resolutions (not yet done)

1. **Typed agent for CI** — start the agent with `-r <refs.xml>` registering
   the px4_msgs types (or a Fast-DDS XML with the `VehicleOdometry` /
   `OffboardControlMode` TypeObjects), then the stub↔companion round-trip is
   CI-able. Needs generating correct DDS type XML from the PX4 `.msg` tree.
2. **PX4 SITL in CI** — run the bring-up in
   `docs/reference/px4-xrce-companion.md` and assert against real `/fmu/out/*`.
   Heavy; gate on SITL availability (`nros_tests::skip!`).
3. **Emit TypeObject from `nros-rmw-xrce`** — have the backend send full type
   information (`create_topic_xml` / binary `TypeObject`) so a bare agent can
   match arbitrary types. Largest change; benefits all non-built-in types, not
   just px4.
