cmake_minimum_required(VERSION 3.22)
project(cpp_add_server_pkg VERSION 0.1.0 LANGUAGES C CXX)

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

# phase-263 A1 (services, C++) — typed AddTwoInts server using the TYPED bind_service<Svc,…>
# (generated example_interfaces C++ srv bindings).
nros_find_interfaces(LANGUAGE CPP SKIP_INSTALL)

nano_ros_node_register(
    NAME    add_server
    CLASS   cpp_add_server_pkg::AddServer
    SOURCES src/AddServer.cpp)

# Guard with if(TARGET) (Zephyr whole-archives the generated interface; see talker_pkg).
if(TARGET example_interfaces__nano_ros_cpp)
    target_link_libraries(cpp_add_server_pkg_add_server_component
        PUBLIC example_interfaces__nano_ros_cpp)
endif()
