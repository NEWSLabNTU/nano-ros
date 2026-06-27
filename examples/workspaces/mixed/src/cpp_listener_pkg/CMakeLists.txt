cmake_minimum_required(VERSION 3.22)
project(cpp_listener_pkg VERSION 0.1.0 LANGUAGES C CXX)

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

nano_ros_node_register(
    NAME    listener
    CLASS   cpp_listener_pkg::Listener
    SOURCES src/Listener.cpp)

# phase-263 C2c-zephyr — guard with `if(TARGET)`: on Zephyr the generated interface is
# whole-archived into `app` (the Zephyr generator), NOT exposed as a `std_msgs__nano_ros_cpp`
# lib, so an unconditional link becomes an unresolved `-lstd_msgs__nano_ros_cpp`; the component
# gets the headers via the `app` include mirror instead. (Same fix as the cpp workspace.)
if(TARGET std_msgs__nano_ros_cpp)
    target_link_libraries(cpp_listener_pkg_listener_component
        PUBLIC std_msgs__nano_ros_cpp)
endif()
