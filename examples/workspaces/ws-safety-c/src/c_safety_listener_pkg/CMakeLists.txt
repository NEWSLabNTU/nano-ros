cmake_minimum_required(VERSION 3.22)
project(c_safety_listener_pkg VERSION 0.1.0 LANGUAGES C CXX)

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

# Generates the std_msgs C bindings (serialize/deserialize); the auto-link
# sidecar links the generated `std_msgs__nano_ros_c` interface lib.
nros_find_interfaces(LANGUAGE C SKIP_INSTALL)

# C safety-listener component: registers a validated subscription on /chatter
# (via nros_cpp_subscription_register_validated), validates the E2E CRC on
# each message, and republishes the running CRC-valid count on /safe_ok via
# the generated std_msgs/Int32 serializer.
# Requires NANO_ROS_SAFETY_E2E=ON (from [system].features = ["safety"]).
nano_ros_node_register(
    NAME     safe_listener
    CLASS    c_safety_listener_pkg::Listener
    LANGUAGE C
    TYPED
    SOURCES  src/SafetyListener.c)
