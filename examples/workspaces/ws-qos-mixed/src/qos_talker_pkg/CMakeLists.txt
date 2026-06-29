cmake_minimum_required(VERSION 3.22)
project(qos_talker_pkg VERSION 0.1.0 LANGUAGES C CXX)

set(CMAKE_C_STANDARD 11)
set(CMAKE_C_STANDARD_REQUIRED ON)

if(NOT COMMAND nano_ros_workspace_pkg_guard)
    if(NOT NANO_ROS_ROOT)
        message(FATAL_ERROR
            "nano-ros: set -DNANO_ROS_ROOT=<path-to-nano-ros> for "
            "standalone builds, or build via the workspace root.")
    endif()
    include("${NANO_ROS_ROOT}/cmake/NanoRosWorkspace.cmake")
endif()
nano_ros_workspace_pkg_guard()

# Typed component (RFC-0043): the raw `/chatter` publisher carries the type name
# as a string and hand-encodes the CDR payload (std_msgs/Int32). The
# differentiator vs the committed c_talker_pkg is the QoS: this declares a
# NON-DEFAULT profile (reliable + transient-local + keep-last-10) in code rather
# than `nros_c_qos_default()` (see src/QosTalker.c).
nano_ros_node_register(
    NAME     qos_talker
    CLASS    qos_talker_pkg::QosTalker
    LANGUAGE C
    TYPED
    SOURCES  src/QosTalker.c)
