cmake_minimum_required(VERSION 3.22)
project(riscv64_threadx_c_listener VERSION 0.1.0 LANGUAGES C CXX ASM)

set(CMAKE_C_STANDARD 11)
set(CMAKE_C_STANDARD_REQUIRED ON)

set(NANO_ROS_PLATFORM threadx)
set(NROS_RMW "zenoh" CACHE STRING
    "Active RMW (zenoh|dds|xrce|cyclonedds) — selects the backend linked into this example.")
set(NANO_ROS_RMW "${NROS_RMW}")
set(NANO_ROS_BOARD    riscv64-qemu)
add_subdirectory("${CMAKE_CURRENT_SOURCE_DIR}/../../../.." nano_ros)

nros_generate_interfaces(std_msgs "msg/Int32.msg" LANGUAGE C SKIP_INSTALL)
nano_ros_read_config("${CMAKE_CURRENT_SOURCE_DIR}/nros.toml")
nano_ros_generate_config_header(
    "${CMAKE_CURRENT_SOURCE_DIR}/nros.toml"
    "${CMAKE_CURRENT_BINARY_DIR}/include/nros/app_config.h")

set(_app_sources src/main.c)
if(NROS_RMW STREQUAL "cyclonedds")
    # CycloneDDS needs the std_msgs/Int32 type descriptor registered
    # explicitly (the backend's weak nros_rmw_cyclonedds_register_app_descriptors
    # is a no-op); mirrors the talker. Without it find_descriptor returns
    # null and subscriber creation fails. Phase 177.28.
    list(APPEND _app_sources src/cyclonedds_app.c)
endif()

add_executable(riscv64_threadx_c_listener ${_app_sources})
target_include_directories(riscv64_threadx_c_listener PRIVATE
    "${CMAKE_CURRENT_BINARY_DIR}/include")
target_link_libraries(riscv64_threadx_c_listener PRIVATE
    std_msgs__nano_ros_c
    NanoRos::NanoRos)
nros_platform_link_app(riscv64_threadx_c_listener)
nano_ros_link_rmw(riscv64_threadx_c_listener RMW "${NROS_RMW}")
