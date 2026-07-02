cmake_minimum_required(VERSION 3.22)
project(telem_pkg VERSION 0.1.0 LANGUAGES C CXX)

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

# rclcpp-shape low-tier telemetry node (RFC-0047, issue #124). Publishes a
# counter on /telem every 100 ms as an IS-A-node (nros::ComponentNode
# subclass). SHAPE rclcpp; tier binding via bind_node_name_sched.
nano_ros_node_register(
    NAME     telem
    CLASS    telem_pkg::Telem
    HEADER   telem_pkg/Telem.hpp
    SHAPE    rclcpp
    CALLBACK_GROUPS telem
    SOURCES  src/Telem.cpp)

# `nros_find_interfaces` declares INTERFACE libs (`std_msgs__nano_ros_cpp`)
# carrying generated headers' include dirs. Guard with `if(TARGET)`: some
# embedded generators fold the generated interface into the app target
# instead of exposing a `<pkg>__nano_ros_cpp` lib.
if(TARGET std_msgs__nano_ros_cpp)
    target_link_libraries(telem_pkg_telem_component
        PUBLIC std_msgs__nano_ros_cpp)
endif()
