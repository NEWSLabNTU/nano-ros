---
id: 1293
title: "On the Rust Cyclone path an EMPTY message has no descriptor, so lifecycle
  services (and any empty-request service) cannot be created"
status: resolved
type: bug
area: rmw
severity: medium
related: [issue-1268, phase-480]
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

## Resolution

Resolved 2026-10-06 (phase-480 W4). Both halves moved together, as the issue
asked, and the serializer half turned out to be wider than Cyclone.

**The wire, measured first.** `rclpy.serialization` on humble, `std_msgs/Empty`:

| RMW | writes | the encapsulation alone (`00 01 00 00`) |
| --- | --- | --- |
| `rmw_zenoh_cpp` | `00 01 00 00 00` + pad | refused: "Not enough memory in the buffer stream" |
| `rmw_cyclonedds_cpp` | `00 01 00 00 00` | refused: "rmw_serialize: invalid data size" |
| `rmw_fastrtps_cpp` | `00 01 00 00 00` + pad | refused (Fast CDR exception) |

nano-ros wrote zero bytes for an empty message on EVERY RMW and in every
language (Rust, C, and the C++ FFI exports). So the defect was not only "no
Cyclone descriptor": any empty message nano-ros sent (`std_msgs/Empty`, a
`std_srvs/Empty` request or response, a `Trigger` request) was undecodable by a
stock peer on zenoh, Cyclone and XRCE alike. Receiving worked, because our
deserializer read nothing and ignored the stock byte.

**The fix.** One spelling, `nros_serdes::schema::EMPTY_STRUCT_MEMBER` /
`EMPTY_STRUCT_FIELDS`:

- codegen emits that member as the `Message::FIELDS` of an empty struct
  (`generator/common.rs`) and uses it for the derived size bound
  (`schema_value.rs`), so `MAX_SERIALIZED_SIZE_*`, the C/C++ `SERIALIZED_SIZE_MAX`
  and the Cyclone descriptor all describe the byte;
- every pack's empty arm writes and reads it: nros Rust (message, service,
  action), C (message, service, action), C++ FFI exports (serialize, publish,
  deserialize). The read is strict, as every stock typesupport is;
- `DescriptorBuilder` still refuses an EMPTY slice: that now means a crate
  generated before this fix, whose serializer writes no byte, and padding only
  the descriptor is the half-fix this issue warned about;
- `create_lc_srv` registers both halves, and `create_lc_pub` registers
  `TransitionEvent` (issue 1587's publisher had the same gap);
- the Cyclone type sizing counts the lifecycle family (9 types) when the
  bringup declares it;
- `NROS_CODEGEN_VERSION` 8 -> 9, floor unchanged at 2: a version-8 tree names
  nothing withdrawn and keeps its old empty-message behaviour until it is
  regenerated.

**Measured, live.** `examples/native/rust/lifecycle-node` built
`--features lifecycle-services,rmw-cyclonedds`, against a stock humble
`ros2` CLI on `rmw_cyclonedds_cpp`, both pinned to loopback:

| tree | result |
| --- | --- |
| before (origin/main) | the image panics at boot: `Failed to register lifecycle node: Transport(Unsupported)`; `ros2 lifecycle get` says `Node not found` |
| after | `ros2 service list` shows all five services; `ros2 lifecycle get` -> `unconfigured [1]`; `ros2 service call .../get_available_states` (an EMPTY request) returns five states; `lifecycle set configure` -> `Transitioning successful`, then `get` -> `inactive [2]` |

One of three runs of the after-tree saw an empty `ros2 service list` and a silent
first `get` in its first 3 s of discovery, and the later calls in that run
succeeded. That is discovery timing in the hand probe, not a failure of the
services.

**Tests that fail without the fix:**
`nros-rmw-cyclonedds/tests/infra_service_descriptors.rs` now builds all nine
lifecycle types, and asserts that `GetStateRequest`'s schema IS
`EMPTY_STRUCT_FIELDS`, that it serializes to `00 01 00 00 00`, and that the
encapsulation alone is refused. On the pre-fix tree the first fails with
`EmptySchema` and the second with a 4-byte payload. The codegen unit test
`empty_request_schema_emits_only_the_rosidl_padding_member` pins the emitter.

**Not measured:**

- A `std_srvs/Trigger` or `std_srvs/Empty` server in a Rust Cyclone image. No
  in-tree image serves one. The lifecycle `get_state` / `get_available_states`
  services are the same shape (empty request, non-empty reply), and those were
  measured.
- A nano-ros CLIENT sending an empty request to a stock server, and a nano-ros
  publisher of `std_msgs/Empty` on zenoh or XRCE. The bytes are pinned by the unit
  test and the stock deserializer's rule by the table above, but the pair was
  not run live.
- No interop cell was added; the live run above is a hand probe. A
  `(Linux, Rust, Cyclonedds, Lifecycle)` cell would need a fixture row and an
  `interop::CELLS` entry.
