cmake_minimum_required(VERSION 3.22)
project(cpp_service_server LANGUAGES C CXX)

set(CMAKE_CXX_STANDARD 14)
set(CMAKE_CXX_STANDARD_REQUIRED ON)

# Phase 118.B.3 — collapsed-shape C++ service-server. RMW selected at
# cmake configure time via `-DNROS_RMW=<rmw>` (default zenoh).
set(NANO_ROS_PLATFORM posix)
set(NROS_RMW "zenoh" CACHE STRING
    "Active RMW (zenoh|dds|xrce|cyclonedds) — selects the backend linked into this example.")
set(NANO_ROS_RMW "${NROS_RMW}")
add_subdirectory("${CMAKE_CURRENT_SOURCE_DIR}/../../../.." nano_ros)

include("${CMAKE_CURRENT_SOURCE_DIR}/../../../../cmake/compat/NrosRclcppCompat.cmake")

find_package(std_msgs REQUIRED)
find_package(example_interfaces REQUIRED)



add_executable(cpp_service_server src/main.cpp)
target_link_libraries(cpp_service_server PRIVATE
    example_interfaces::example_interfaces
    NanoRos::NanoRosCpp)
nros_platform_link_app(cpp_service_server)
