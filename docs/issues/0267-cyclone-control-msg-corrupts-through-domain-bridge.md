---
id: 267
title: "nano-ros-published Control msg deserializes as garbage after ros2 domain_bridge generic republish (direct typed echo clean)"
status: open
type: bug
severity: high
area: rmw
---

## Symptom (simple-autoware-safety-island demo, 2026-07-24)

The island (cyclone RMW) publishes `autoware_control_msgs/Control`
(`longitudinal.acceleration = -2.5`). A humble `ros2 topic echo` subscribing
DIRECTLY (same domain) decodes it correctly — proven repeatedly. But when
`ros2 domain_bridge` (humble branch, GenericSubscription/GenericPublisher
serialized passthrough) rebroadcasts the SAME topic into another domain, the
downstream typed subscriber decodes garbage:

    longitudinal.acceleration = 2677354240.0   (≈ 0x9F99999A — bytes of the
    real payload shifted; -2.5f is 0xC0200000, 0.3f is 0x3E99999A)

`autoware_adapi_v1_msgs/MrmState` (flat: Time + 2×uint16) crosses the same
bridge CLEAN. `Control` nests Lateral/Longitudinal each with TWO
builtin_interfaces/Time members — the shifted-float signature points at a
CDR alignment divergence in the nested-struct layout that a typed cyclone
reader tolerates (or realigns) but a serialized-passthrough rebroadcast
preserves verbatim into a payload the next typed reader mis-walks.

Live impact: the demo's sim-side vehicle_cmd_gate consumed the garbage
emergency command and accelerated the vehicle to the 50 m/s cap.

## Repro sketch

1. Island (nano-ros cyclone, domain 2) publishing Control.
2. `ros2 run domain_bridge domain_bridge` with a 2→1 row for the topic.
3. Domain 1: `ros2 topic echo /system/emergency/control_cmd` → garbage;
   domain 2 direct echo → clean.

## Suspect

nano-ros CDR serializer's padding for nested structs w/ Time members
(4+4 bytes) vs rosidl's XCDR1 alignment rules — a typed reader may
resynchronize while byte-level rebroadcast exposes the divergence. Compare
`nros-serdes` output byte-for-byte with `rmw_cyclonedds_cpp` for
autoware_control_msgs/Control; check encapsulation header + 8-byte-alignment
of float64s... (Control has only float32 — suspect the bool tail of Lateral
(`is_defined_steering_tire_rotation_rate`) + struct padding before
Longitudinal).

## Workaround in the demo

Single-bridge topology (fault = whole-bridge pause) instead of the split
forward/reverse topology that would have kept the island's commands flowing
through the rebroadcast path.
