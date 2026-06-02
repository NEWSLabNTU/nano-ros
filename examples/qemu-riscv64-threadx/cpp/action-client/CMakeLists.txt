cmake_minimum_required(VERSION 3.22)
project(riscv64_threadx_cpp_action_client VERSION 0.1.0 LANGUAGES C CXX ASM)

set(CMAKE_C_STANDARD 11)
set(CMAKE_C_STANDARD_REQUIRED ON)
set(CMAKE_CXX_STANDARD 14)
set(CMAKE_CXX_STANDARD_REQUIRED ON)

set(NANO_ROS_PLATFORM threadx)
set(NROS_RMW "zenoh" CACHE STRING
    "Active RMW (zenoh|dds|xrce|cyclonedds) — selects the backend linked into this example.")
set(NANO_ROS_RMW "${NROS_RMW}")
set(NANO_ROS_BOARD    riscv64-qemu)
add_subdirectory("${CMAKE_CURRENT_SOURCE_DIR}/../../../.." nano_ros)

include("${CMAKE_CURRENT_SOURCE_DIR}/../../../../cmake/compat/NrosRclcppCompat.cmake")
find_package(example_interfaces REQUIRED)

add_executable(riscv64_threadx_cpp_action_client src/main.cpp)
target_link_libraries(riscv64_threadx_cpp_action_client PRIVATE
    example_interfaces::example_interfaces
    NanoRos::NanoRosCpp)
nros_platform_link_app(riscv64_threadx_cpp_action_client)
nano_ros_link_rmw(riscv64_threadx_cpp_action_client RMW "${NROS_RMW}")
