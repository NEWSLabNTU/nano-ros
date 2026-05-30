# No-op Find-stub for rcl_interfaces — Phase 209.B (NrosRclcppCompat).
# nano-ros doesn't ship this ROS 2 package; the surface a ported source needs
# (message types, rcl handles) is satisfied through NanoRos::NanoRosCpp + nros
# codegen. The find_package call only needs to succeed.
set(rcl_interfaces_FOUND TRUE)
