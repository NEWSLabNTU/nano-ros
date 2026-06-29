cmake_minimum_required(VERSION 3.22)
project(qos_listener_pkg VERSION 0.1.0 LANGUAGES C CXX)

set(CMAKE_CXX_STANDARD 14)
set(CMAKE_CXX_STANDARD_REQUIRED ON)

if(NOT COMMAND nano_ros_workspace_pkg_guard)
    if(NOT NANO_ROS_ROOT)
        message(FATAL_ERROR
            "nano-ros: set -DNANO_ROS_ROOT=<path-to-nano-ros> for "
            "standalone builds, or build via the workspace root.")
    endif()
    include("${NANO_ROS_ROOT}/cmake/NanoRosWorkspace.cmake")
endif()
nano_ros_workspace_pkg_guard()

nros_find_interfaces(LANGUAGE CPP SKIP_INSTALL)

# Typed component (RFC-0043): a raw zero-copy `/chatter` subscription keyed on
# `std_msgs::msg::Int32::TYPE_NAME`. The differentiator vs the committed
# listener_pkg is the QoS: this declares the SAME non-default profile (reliable +
# transient-local + keep-last-10) as the talker so the QoS-matched endpoints
# connect (see src/QosListener.cpp).
nano_ros_node_register(
    NAME    qos_listener
    CLASS   qos_listener_pkg::QosListener
    SOURCES src/QosListener.cpp)

# `nros_find_interfaces` declares the `std_msgs__nano_ros_cpp` INTERFACE lib (for
# `Int32::TYPE_NAME`); guard with `if(TARGET)` (the Zephyr generator whole-archives
# the interface into `app` instead).
if(TARGET std_msgs__nano_ros_cpp)
    target_link_libraries(qos_listener_pkg_qos_listener_component
        PUBLIC std_msgs__nano_ros_cpp)
endif()
