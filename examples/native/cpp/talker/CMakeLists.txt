cmake_minimum_required(VERSION 3.22)
project(cpp_talker LANGUAGES C CXX)

set(CMAKE_CXX_STANDARD 14)
set(CMAKE_CXX_STANDARD_REQUIRED ON)

# Phase 118.B.3 / 210.E.3.a — collapsed-shape C++ talker. RMW selected
# at cmake configure time via `-DNROS_RMW=<rmw>` (default zenoh). The
# msg bindings now come in via the canonical `find_package(<pkg>)` +
# smart Find-stub flow (Phase 210.A.2) instead of legacy
# `nros_generate_interfaces(<pkg>)` calls.
set(NANO_ROS_PLATFORM posix)
set(NROS_RMW "zenoh" CACHE STRING
    "Active RMW (zenoh|dds|xrce|cyclonedds) — selects the backend linked into this example.")
set(NANO_ROS_RMW "${NROS_RMW}")
add_subdirectory("${CMAKE_CURRENT_SOURCE_DIR}/../../../.." nano_ros)

# Pull the rclcpp source-compat surface — defines the smart Find-stub
# that resolves `find_package(<msg-pkg>)` through the layered search
# path (workspace > AMENT_PREFIX_PATH > bundled), runs codegen, and
# emits `<pkg>::<pkg>` IMPORTED targets.
include("${CMAKE_CURRENT_SOURCE_DIR}/../../../../cmake/compat/NrosRclcppCompat.cmake")

find_package(std_msgs REQUIRED)

add_executable(cpp_talker src/main.cpp)
target_link_libraries(cpp_talker PRIVATE
    std_msgs::std_msgs
    NanoRos::NanoRosCpp)
nros_platform_link_app(cpp_talker)
