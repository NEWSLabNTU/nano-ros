# No-op find module for ament_cmake_core — phase-209 B; part of nano-ros's CMake package (phase-482 W1).
# nano-ros doesn't ship this ROS 2 package; the surface a ported source needs
# (message types, rcl handles) is satisfied through NanoRos::NanoRosCpp + nros
# codegen. The find_package call only needs to succeed.
set(ament_cmake_core_FOUND TRUE)
