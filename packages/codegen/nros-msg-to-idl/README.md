# nros-msg-to-idl

Pure-Rust port of `scripts/cyclonedds/msg_to_cyclone_idl.py`.

Converts a ROS 2 `.msg` file (with optional `.srv` service-header injection)
to a Cyclone-DDS-shaped IDL string that, when fed through `idlc -t -l c`,
produces a `dds_topic_descriptor_t::m_typename` exactly equal to
`<pkg>::<msg|srv>::dds_::<Type>_` — the same string stock
`rmw_cyclonedds_cpp` emits. A nano-ros publisher and an `rclcpp` subscriber
on the same topic name then match without translation.

Output is **byte-identical** to the python script for the subset of `.msg`
syntax used by the bundled `rcl_interfaces`, `builtin_interfaces`,
`std_msgs`, `geometry_msgs` and `sensor_msgs` messages (Phase 212.K.3).

## Library

```rust
use nros_msg_to_idl::{Converter, ConvertError};

let idl = Converter::new("std_msgs", "Int32")
    .convert(include_str!(".../Int32.msg"))?;
```

## CLI

```sh
nros-msg-to-idl --package std_msgs --message Int32 path/to/Int32.msg
```

Drop-in replacement for the python script's `--interface` mode (one
file at a time; the python multi-interface form is **not** mirrored
— the Cyclone wrapper sys crate invokes the library directly).
