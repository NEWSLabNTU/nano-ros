---
id: 280
title: "Cyclone type-descriptor registry cap 64 overflowed SILENTLY — link-order-last ts archive dropped, boot failed as TransportError(-100)"
status: resolved
type: bug
severity: high
area: rmw-cyclonedds
---

## Finding (autoware-safety-island-example ports, 2026-07-24 — porting-notes 16)

`kMaxRegisteredTypes = 64` in descriptors.cpp; the 4-node island registers
~86 types (std_msgs + geometry_msgs full sets alone ~60). Registrations
past the cap were dropped WITHOUT any diagnostic — whichever typesupport
archive was link-order last (tier4) lost its types, and the failure
surfaced at boot as `create_publisher` UNSUPPORTED(-5) wrapped in
TransportError(-100), nowhere near the cause. Diagnosis red herring: the
`NROS_CYCLONEDDS_MAX_TYPES` env knob is a DIFFERENT registry.

## Resolution (same-day, 2026-07-24)

Cap raised to 256 + `NROS_CYCLONEDDS_MAX_DESCRIPTOR_TYPES` override define.
Residual hardening worth keeping in mind: overflow should PANIC or log at
registration time, not drop silently — the silent path is what cost the
debugging session. Filed retroactively for the record trail.
