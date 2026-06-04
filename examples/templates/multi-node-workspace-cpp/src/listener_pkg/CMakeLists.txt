cmake_minimum_required(VERSION 3.22)
# §212.L.4 — class prefix must equal PROJECT_NAME.
project(listener_pkg VERSION 0.1.0 LANGUAGES C CXX)

set(CMAKE_CXX_STANDARD 14)
set(CMAKE_CXX_STANDARD_REQUIRED ON)

# Phase 219.I — workspace builds inherit the helpers via the parent
# root; standalone solo builds (this dir) bootstrap via the include
# below. Same shape `nros new --component --lang cpp` emits.
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

# Phase 212.L.9 — declarative Node pkg shape. No add_executable, no
# `main()`; the linked Entry pkg's BSP-generated runtime (post-219.D)
# owns the entry point, executor init, and the spin loop.
nano_ros_node_register(
    NAME    listener
    CLASS   listener_pkg::Listener
    SOURCES src/Listener.cpp
    DEPLOY  native)

# `nros_find_interfaces` declares INTERFACE libs (`std_msgs__nano_ros_cpp`)
# carrying generated headers' include dirs. Entry pkg auto-links; Node
# pkg does not (Phase 219 review Gap 4 — pending 219.J auto-link from
# launch metadata).
target_link_libraries(listener_pkg_listener_component
    PUBLIC std_msgs__nano_ros_cpp)
