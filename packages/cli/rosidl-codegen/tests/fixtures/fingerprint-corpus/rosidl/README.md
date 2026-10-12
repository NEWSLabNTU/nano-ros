Stock ROS 2 Humble interface definitions (Apache-2.0, `common_interfaces` /
`rcl_interfaces`), copied verbatim. phase-492 W7: the `rosidl` arm of the
fingerprint corpus emits them under their REAL package names, so the committed
goldens in `../expected/rosidl/` are laid out as rosidl lays them out
(`std_msgs/msg/string.h`) and the C compile and run probes in
`packages/api/nros-c/tests/` can include them as a ported file would.
