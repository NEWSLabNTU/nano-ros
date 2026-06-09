# cpp-port-minimal-publisher — Phase 209.G iter 2.
#
# Vendors the canonical ROS 2 "minimal publisher" tutorial node UNMODIFIED and
# builds it against nano-ros via the 209.A-D compat surface. The CMakeLists.txt
# nano-ros glue (the three lines below `# 1)`/`# 2)`/`# 3)`) is the only
# delta from a stock ROS 2 ament_cmake_auto package.

cmake_minimum_required(VERSION 3.22)
project(minimal_publisher LANGUAGES C CXX)
set(CMAKE_CXX_STANDARD 14)
set(CMAKE_CXX_STANDARD_REQUIRED ON)

# 1) Pull nano-ros (NanoRos::NanoRos / NanoRos::NanoRosCpp / nros_find_interfaces).
set(NANO_ROS_PLATFORM posix)
set(NROS_RMW "zenoh" CACHE STRING "Active RMW (zenoh|cyclonedds|xrce).")
set(NANO_ROS_RMW "${NROS_RMW}")
add_subdirectory("${CMAKE_CURRENT_SOURCE_DIR}/../../.." nano_ros)

# 2) rclcpp / ament_cmake_auto / rclcpp_components source-compat.
include("${CMAKE_CURRENT_SOURCE_DIR}/../../../cmake/compat/NrosRclcppCompat.cmake")

# 3) Generate the message bindings the ported source includes. package.xml is the
#    single source of truth for the msg deps; nros_find_interfaces() resolves the
#    transitive closure (builtin_interfaces ← std_msgs) from it.
nros_find_interfaces(LANGUAGE CPP SKIP_INSTALL)

# --- below: unmodified stock-ROS-2 ament_cmake_auto pattern ---

find_package(ament_cmake_auto REQUIRED)
find_package(rclcpp REQUIRED)
ament_auto_find_build_dependencies()

ament_auto_add_executable(minimal_publisher src/minimal_publisher.cpp)
ament_target_dependencies(minimal_publisher rclcpp)
target_link_libraries(minimal_publisher PRIVATE std_msgs__nano_ros_cpp)

ament_auto_package()
