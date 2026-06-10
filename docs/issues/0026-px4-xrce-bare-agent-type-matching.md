---
id: 26
title: cross-session px4_msgs matching needs a typed agent — bare MicroXRCEAgent matches non-built-in types only intra-session
status: open
type: limitation
area: rmw-xrce
related: [phase-233, rfc-0039]
---

> **Refined (Phase 233.4).** It is **cross-session** (two XRCE sessions / two
> DDS participants) discovery that fails on a bare agent for non-built-in
> types — *not* the type itself. A **single-session** pub+sub of `px4_msgs`
> round-trips fine against a bare agent (matched intra-participant); this is
> now CI-covered by `nros-tests::px4_xrce` driving `px4-stub` in loopback
> mode. The companion ↔ PX4 path is inherently cross-session, so it still
> needs PX4's typed agent (or `-r refs`). See the refined analysis below.


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
`TypeObject` / IDL. Within **one** XRCE session the agent matches the writer
and reader under a single participant without consulting a type registry, so
any type — `px4_msgs` included — round-trips. Across **two** sessions the
agent's DDS layer must discover and type-check two participants; without a
`TypeObject` it can only match types its own plugin already knows. The bundled
build resolves built-in ROS types but not `px4_msgs`. A real PX4 deployment
runs the agent **with px4_msgs typesupport** (built/run alongside `px4_msgs`),
so PX4's `uxrce_dds_client` and an nano-ros companion (two sessions) match
there.

Verified by controlled A/B: `examples/native/rust/custom-msg` (a non-built-in
custom type, single session) round-trips on a bare agent; `std_msgs/Int32`
(built-in) round-trips cross-session; `px4_msgs` round-trips single-session
(`px4-stub` loopback) but not cross-session (`px4-stub` → `offboard-companion`)
on a bare agent.

## Impact

- The `offboard-companion` example is correct for a real PX4 agent; the
  publish (`/fmu/in/*`) and subscribe (`/fmu/out/*`) wiring + the `px4()` QoS
  profile are exercised, and the session/entity creation succeeds.
- The full `px4_msgs` serialize → agent → deserialize round-trip **is**
  CI-covered against a bare agent via the single-session loopback
  (`nros-tests::px4_xrce` → `px4-stub` `PX4_STUB_LOOPBACK=1`). What a bare
  agent cannot do is the **cross-session** companion ↔ PX4 receive; that needs
  a typed agent (PX4 SITL or `-r refs`).

## Resolutions (not yet done)

1. **Typed agent for CI** — start the agent with `-r <refs.xml>` registering
   the px4_msgs types so the cross-session stub↔companion round-trip becomes
   CI-able. A first attempt (a Fast-DDS `<types>` profiles XML for
   `VehicleOdometry` / `OffboardControlMode` passed via `-r`) did **not** make
   them match; the bundled agent ships no logging, so it could not be
   confirmed whether the XML was loaded/registered. Needs the exact agent refs
   format (likely full `TypeObject`/IDL, not just a `<types>` member list).
   *(The single-session loopback already covers the serialization round-trip,
   so this is only needed to exercise cross-participant discovery in CI.)*
2. **PX4 SITL in CI** — run the bring-up in
   `docs/reference/px4-xrce-companion.md` and assert against real `/fmu/out/*`.
   Heavy; gate on SITL availability (`nros_tests::skip!`).
3. **Emit TypeObject from `nros-rmw-xrce`** — have the backend send full type
   information (`create_topic_xml` / binary `TypeObject`) so a bare agent can
   match arbitrary types. Largest change; benefits all non-built-in types, not
   just px4.
