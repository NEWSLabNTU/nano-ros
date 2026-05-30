# No-op Find-stub for action_msgs — Phase 209.B (NrosRclcppCompat).
# nano-ros doesn't ship this ROS 2 package; the surface a ported source needs
# (message types, rcl handles) is satisfied through NanoRos::NanoRosCpp + nros
# codegen. The find_package call only needs to succeed.
set(action_msgs_FOUND TRUE)
