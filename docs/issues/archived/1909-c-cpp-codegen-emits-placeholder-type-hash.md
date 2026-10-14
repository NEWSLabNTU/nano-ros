---
id: 1909
title: "C and C++ message codegen emits an all-zero RIHS01 placeholder for Iron/Jazzy, so a C or C++ image built for those editions matches no ROS 2 peer"
status: resolved
type: bug
severity: high
area: [cli, codegen, rmw-zenoh]
related: [issue-1893, issue-1913, phase-304]
found: 2026-10-15
resolved: 2026-10-15
---

## Measured

Found while verifying issue 1893's fix on the C road. `examples/linux/c/talker`
configured with `-DNROS_ROS_EDITION=jazzy` now builds with
`--features=ros-jazzy,rmw-zenoh,nros-rmw-zenoh/ros-jazzy,…`, so the zenoh
backend keys the RIHS01 form. Against a Jazzy `ros2 topic echo /chatter`
(`nano-ros-ros:jazzy`, `rmw_zenoh_cpp` 0.2.11, through a Jazzy `rmw_zenohd`) it
delivered **0** samples while printing `Publishing: 'Hello World: N'`.
`ros2 topic info -v --no-daemon /chatter`:

```
Topic type hash: RIHS01_0000000000000000000000000000000000000000000000000000000000000000
Endpoint type: PUBLISHER
Topic type hash: RIHS01_df668c740482bbd48fb39d76a70dfd4bd59db1288021743503259e948f6b1a18
Endpoint type: SUBSCRIPTION
```

The generated C header says the same:
`tmp/…/nano_ros_c/std_msgs/msg/string.h:129: return "RIHS01_0000…0000";` (64
headers in that build carry it).

## Cause

`rosidl_codegen::RosEdition::type_hash()` returns a placeholder for Iron+, and
says so: "the value is a placeholder (all-zero sha256) until phase-304 W1
computes the real REP-2011 hash". The Rust generator stopped using it —
`rosidl-bindgen/src/generator.rs` computes the real hash per message, service
and action through `ResolvedMessage` / `ResolvedService` / `ResolvedAction`, and
`nros generate-px4-msgs` does too (the PX4 bridge measured the real hash on the
wire under 1893). The C and C++ generators did not:
`packages/cli/cargo-nano-ros/src/lib.rs` takes `edition.type_hash()` at three
sites (`generate_c_from_args_file` and the two C++ paths) and writes it into
every type.

So every C or C++ image generated for iron or jazzy advertises, and keys its
data on, a hash no ROS 2 peer computes. Before 1893 this was hidden: the zenoh
backend never received the edition on the C/C++ road and keyed Humble's
`TypeHashNotSupported`, which matched no Iron+ peer either.

## What would close it

Compute the hash in the C and C++ generators the way `rosidl-bindgen` does,
over the same resolver (the args file already names the dependency packages),
and hard-fail on an unresolvable nested type rather than emit a plausible
wrong one. Acceptance is a C and a C++ talker built for jazzy delivering to a
Jazzy subscriber (`just ros_editions image jazzy` gives one on a Humble host),
plus a golden test against a hash Jazzy ships (`/opt/ros/jazzy/share/std_msgs/msg/String.json`).

## Resolution (2026-10-15)

`cargo-nano-ros` now computes the hash for the C and C++ emitters the way
`rosidl-bindgen` does: `c_message_type_hash` (`ResolvedMessage`) per message and
`c_service_type_hash` (`ResolvedService`) per service, over the nested-type
lookup each of the three emitter entry points already built for size bounds
(`nested_msg_lookup` — same-package siblings, then the ament / bundled index).
An unresolvable nested type is a hard error. Humble keeps its sentinel.

A service is keyed on the SERVICE's own hash, and that is what both runtimes
read: the C type support's `.type_hash`, and the C++ `Svc::Request::TYPE_HASH`
that `create_service` / `create_client` hand the FFI. So the service hash goes
into the emitter's one service `type_hash` slot; the C++ `Request` / `Response`
structs carry it rather than their own message hashes, which nothing reads.

**Actions are not fixed here** — issue 1913. The raw action spec the C/C++
APIs reach carries ONE hash for five endpoints that a Jazzy peer keys on five
different hashes, so they keep the placeholder until that ABI carries them.

Golden test `c_cpp_type_hashes_match_what_jazzy_ships` pins `std_msgs/String`,
`std_msgs/Header` (cross-package nested `builtin_interfaces/Time`) and
`example_interfaces/srv/AddTwoInts` against Jazzy's shipped `.json` hashes, and
asserts the hard error on an unresolvable nested type.

**Receipts** (`nano-ros-ros:jazzy`, `rmw_zenoh_cpp` 0.2.11, Jazzy router and
peer in the container, nano-ros side on the host, each built with
`-DNROS_ROS_EDITION=jazzy`):

```
examples/linux/c/talker     -> ros2 topic echo /chatter         samples: 19   (was 0)
examples/linux/cpp/talker   -> ros2 topic echo /chatter         data: 'Hello World: N'
examples/linux/c/service-server <- ros2 service call /add_two_ints {a: 2, b: 3}
                                                                 AddTwoInts_Response(sum=5)
```
