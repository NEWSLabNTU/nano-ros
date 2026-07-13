cmake_minimum_required(VERSION 3.22)
# §212.L.4 — class prefix must equal PROJECT_NAME.
project(listener_pkg VERSION 0.1.0 LANGUAGES C CXX)

set(CMAKE_CXX_STANDARD 14)
set(CMAKE_CXX_STANDARD_REQUIRED ON)

find_package(nano_ros REQUIRED)
find_package(std_msgs REQUIRED)


# Phase 240.2b — typed component (header at include/listener_pkg/Listener.hpp).
nano_ros_add_node(listener CLASS listener_pkg::Listener src/Listener.cpp DEPLOY native)

target_link_libraries(listener_pkg_listener_component
    PUBLIC std_msgs__nano_ros_cpp)
