cmake_minimum_required(VERSION 3.22)
project(nuttx_c_listener VERSION 0.1.0 LANGUAGES C CXX)

set(CMAKE_C_STANDARD 11)
set(CMAKE_C_STANDARD_REQUIRED ON)

set(NANO_ROS_PLATFORM nuttx)
set(NROS_RMW "zenoh" CACHE STRING "Active RMW.")
set(NANO_ROS_RMW "${NROS_RMW}")
set(NANO_ROS_BOARD    nuttx-qemu-arm)
add_subdirectory("${CMAKE_CURRENT_SOURCE_DIR}/../../../.." nano_ros)

include("${CMAKE_CURRENT_SOURCE_DIR}/../../../../cmake/NanoRosNodeRegister.cmake")
nros_generate_interfaces(std_msgs "msg/Int32.msg" LANGUAGE C SKIP_INSTALL)

nano_ros_node_register(
    NAME    listener
    CLASS   nuttx_c_listener::Listener
    SOURCES src/Listener.c ${std_msgs_GENERATED_SOURCES}
    DEPLOY  nuttx)

nano_ros_deploy(TARGET nuttx RMW ${NROS_RMW} DOMAIN_ID 0)

# Phase 214 followup — link interface lib so generated headers
# (e.g. ${pkg}__nano_ros_c-provided std_msgs/std_msgs.h) are on the include
# path for src/*.c sources that #include "std_msgs.h".
target_link_libraries(nuttx_c_listener_listener_component PUBLIC std_msgs__nano_ros_c)
