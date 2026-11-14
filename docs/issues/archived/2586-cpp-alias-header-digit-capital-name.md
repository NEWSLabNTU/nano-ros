---
id: 2586
title: "The ROS-style C++ alias header missed rosidl's name for a digit or acronym before a capital (`float32stamped.hpp`, not `float32_stamped.hpp`)"
status: resolved
type: bug
severity: medium
area: [codegen, cpp]
related: [phase-478, phase-492]
found: 2026-11-15
resolved: 2026-11-15
resolved_in: "the change that filed it"
---

**Home: [phase-478](../../roadmap/phase-478-island-developer-experience-findings.md).**
Mirrors GitHub #3882.

## What

Codegen writes a ROS-style alias header, `<pkg>/<kind>/<name>.hpp`, beside each
prefixed C++ header. A node ported from rclcpp then keeps its includes
unchanged. The alias was named with the C++ surface's `to_snake_case`, which
puts an underscore only after a lower-case letter. rosidl_generator_cpp uses
`convert_camel_case_to_lower_case_underscore`, which also breaks after a digit
and in front of the last capital of an acronym:

| Type | rosidl | nano-ros, before |
| --- | --- | --- |
| `Float32Stamped` | `float32_stamped.hpp` | `float32stamped.hpp` |
| `Float32MultiArrayStamped` | `float32_multi_array_stamped.hpp` | `float32multi_array_stamped.hpp` |
| `UInt8` | `u_int8.hpp` | `uint8.hpp` |
| `TFMessage` | `tf_message.hpp` | `tfmessage.hpp` |

On the island, `#include <tier4_debug_msgs/msg/float32_stamped.hpp>` failed with
`No such file or directory`.

## Fix

The C surface already takes rosidl's names through
`utils::to_rosidl_snake_case` (phase-492 W7). The alias now uses the same rule,
through one name function, `generator::naming::ros_alias_header`. Every
emitter that writes an alias calls it: `cargo-nano-ros`'s msg, srv and action
arms, and `rosidl-bindgen`'s PX4 C++ package. The prefixed header the alias
forwards to keeps the C++ surface's own stem, so no golden moves.

`the_alias_header_takes_rosidls_name` checks 23 names against the output of
rosidl's own function (ROS 2 Humble `rosidl_cmake`). They cover a digit
before a capital, acronyms, an acronym before a word, and the plain cases.

Sweep (every `.hpp` alias name; should print only `naming.rs`):

```
git grep -n 'format!("{}.hpp", to_snake_case' -- 'packages/cli/**/*.rs'
git grep -n 'ros_alias_header' -- 'packages/cli/**/*.rs'
```

The CMake output prediction (`_nros_predict_generated_outputs`) names only the
prefixed header and the C files, so it already agreed.

## Resolution

Fixed as above.
