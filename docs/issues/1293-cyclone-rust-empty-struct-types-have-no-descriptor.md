---
id: 1293
title: "On the Rust Cyclone path an EMPTY message has no descriptor, so lifecycle
  services (and any empty-request service) cannot be created"
status: open
type: bug
area: rmw
severity: medium
related: [issue-1268]
---

## Symptom

Found while fixing issue 1268 and sweeping its class: the services the executor
creates on its own. The six parameter services now register their types and
come up on Cyclone. The five lifecycle services do not. Three of their request
types are EMPTY:

| type | `FIELDS` |
| --- | --- |
| `lifecycle_msgs/srv/GetState_Request` | `&[]` |
| `lifecycle_msgs/srv/GetAvailableStates_Request` | `&[]` |
| `lifecycle_msgs/srv/GetAvailableTransitions_Request` | `&[]` |

The Rust descriptor builder refuses an empty schema outright
(`nros-rmw-cyclonedds/src/dynamic_type.rs`, `BuildError::EmptySchema`), and so
does the C++ bridge behind it (`BridgeError::EmptySchema`). So
`register_type::<GetStateRequest>()` fails, and so does every service whose
request or response is empty. That includes user services: `std_srvs/Trigger`
and `std_srvs/Empty` on a Rust Cyclone image.

## Cause

ROS pads an empty struct. `rosidl_generator_dds_idl` emits
`uint8 structure_needs_at_least_one_member` for it, so on the wire an empty
request is one byte, not zero. The IDL path here does the same:
`scripts/cyclonedds/msg_to_cyclone_idl.py` and `nros-msg-to-idl`'s emitter both
add the member, which is why the C/C++ CMake route (`nros_generate_interfaces`)
has no such gap. The Rust path does not pad in either place:

- the generated schema has `FIELDS = &[]`, and the builder rejects that;
- the generated `Serialize` writes zero payload bytes (`begin_dheader` /
  `end_dheader`, which is no bytes under humble's FINAL extensibility).

So fixing only the descriptor, by synthesizing the one `uint8` member when
`FIELDS` is empty, would give a Cyclone topic whose type says one byte while
our writer sends none. Stock peers would then fail to deserialize. Both halves
have to move together, and the codegen change affects every generated crate.
That is why this was split out of 1268 rather than landing half of it there.

## Fix shape

- `DescriptorBuilder`: an empty schema becomes a single
  `structure_needs_at_least_one_member: uint8` member (the name rosidl uses),
  not an error.
- Codegen: an empty message serializes and deserializes that one byte, on
  every RMW. Check the zenoh and XRCE wire against stock before changing it,
  since those peers use the same rosidl typesupport.
- Then `create_lc_srv` (`nros-node/src/executor/spin.rs`) calls
  `register_type` for both halves, as `create_param_srv` does since 1268.

## Acceptance

- `ros2 lifecycle get /<node>` answers from a Cyclone image.
- A `std_srvs/Trigger` server on a Rust Cyclone image answers `ros2 service call`.
