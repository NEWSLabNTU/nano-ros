cmake_minimum_required(VERSION 3.22)
project(threadx_c_service_server VERSION 0.1.0 LANGUAGES C CXX)

set(CMAKE_C_STANDARD 11)
set(CMAKE_C_STANDARD_REQUIRED ON)

set(NANO_ROS_PLATFORM threadx)
set(NROS_RMW "zenoh" CACHE STRING
    "Active RMW (zenoh|dds|xrce|cyclonedds) — selects the backend linked into this example.")
set(NANO_ROS_RMW "${NROS_RMW}")
set(NANO_ROS_BOARD    threadx-linux)
add_subdirectory("${CMAKE_CURRENT_SOURCE_DIR}/../../../.." nano_ros)

nros_generate_interfaces(example_interfaces "srv/AddTwoInts.srv" LANGUAGE C SKIP_INSTALL)

add_executable(threadx_c_service_server src/main.c)
target_link_libraries(threadx_c_service_server PRIVATE
    example_interfaces__nano_ros_c
    NanoRos::NanoRos)
nros_platform_link_app(threadx_c_service_server)
nano_ros_link_rmw(threadx_c_service_server RMW "${NROS_RMW}")
