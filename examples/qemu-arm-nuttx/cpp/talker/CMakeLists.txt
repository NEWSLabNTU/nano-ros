cmake_minimum_required(VERSION 3.22)
project(nuttx_cpp_talker VERSION 0.1.0 LANGUAGES C CXX)

set(CMAKE_CXX_STANDARD 14)
set(CMAKE_CXX_STANDARD_REQUIRED ON)

set(NANO_ROS_PLATFORM nuttx)
set(NROS_RMW "zenoh" CACHE STRING "Active RMW.")
set(NANO_ROS_RMW "${NROS_RMW}")
set(NANO_ROS_BOARD    nuttx-qemu-arm)
add_subdirectory("${CMAKE_CURRENT_SOURCE_DIR}/../../../.." nano_ros)

include("${CMAKE_CURRENT_SOURCE_DIR}/../../../../cmake/NanoRosNodeRegister.cmake")

# Typed Publisher<Int32> needs the std_msgs C++ binding (Int32 struct +
# TYPE_NAME/ffi_publish). The carrier auto-links the generated interface lib.
nros_find_interfaces(LANGUAGE CPP SKIP_INSTALL)

# Phase 240.6 (RFC-0043) — TYPED carrier: the bootable ELF constructs the typed
# Talker component + `configure(node)` creates a typed publisher + binds the
# real `on_tick` timer (publishes an Int32 counter) on the executor.
nano_ros_node_register(
    NAME      talker
    CLASS     nuttx_cpp_talker::Talker
    LANGUAGE  CPP
    HEADER    Talker.hpp
    TYPED
    SOURCES   src/Talker.cpp
    DEPLOY    nuttx)

target_link_libraries(nuttx_cpp_talker_talker_component
    PUBLIC std_msgs__nano_ros_cpp)

nano_ros_deploy(TARGET nuttx RMW ${NROS_RMW} DOMAIN_ID 0)
