# No-op find module for ament_cmake_auto — phase-209 B; part of nano-ros's CMake package (phase-482 W1).
# nano-ros doesn't ship this ROS 2 package; the surface a ported source needs
# (message types, rcl handles) is satisfied through NanoRos::NanoRosCpp + nros
# codegen. The find_package call only needs to succeed.
# phase-518 W4 — the directory that asks is a ported ament package.
include("${CMAKE_CURRENT_LIST_DIR}/../NanoRosPortedCxx.cmake")
nros_ported_ament_package()
set(ament_cmake_auto_FOUND TRUE)
