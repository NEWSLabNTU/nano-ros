cmake_minimum_required(VERSION 3.22)
project(native_listener_entry VERSION 0.1.0 LANGUAGES C CXX)

set(CMAKE_CXX_STANDARD 14)
set(CMAKE_CXX_STANDARD_REQUIRED ON)

find_package(nano_ros REQUIRED)

# phase-263 B4 (QoS, C++) — single-node LISTENER entry (C++ TYPED carrier). Cross-process
# topology (issue 0096): the auto-link sidecar pulls qos_listener_pkg's component from
# listener.launch.xml.
nano_ros_add_executable(native_listener_entry
    SOURCES src/main.cpp
    BOARD   native
    LAUNCH  "demo_bringup:listener.launch.xml"
    TYPED
    DEPLOY  native)
