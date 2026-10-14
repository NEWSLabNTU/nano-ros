---
id: 1909
title: "C and C++ message codegen emits an all-zero RIHS01 placeholder for Iron/Jazzy, so a C or C++ image built for those editions matches no ROS 2 peer"
status: open
type: bug
severity: high
area: [cli, codegen, rmw-zenoh]
related: [issue-1893, phase-304]
found: 2026-10-15
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
