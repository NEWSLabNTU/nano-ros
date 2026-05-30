# No-op Find-stub for rosidl_runtime_cpp — Phase 209.B (NrosRclcppCompat).
# nano-ros doesn't ship this ROS 2 package; the surface a ported source needs
# (message types, rcl handles) is satisfied through NanoRos::NanoRosCpp + nros
# codegen. The find_package call only needs to succeed.
set(rosidl_runtime_cpp_FOUND TRUE)
