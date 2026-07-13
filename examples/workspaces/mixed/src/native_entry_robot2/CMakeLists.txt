cmake_minimum_required(VERSION 3.22)
project(native_entry_robot2 VERSION 0.1.0 LANGUAGES C CXX)

set(CMAKE_CXX_STANDARD 14)
set(CMAKE_CXX_STANDARD_REQUIRED ON)

find_package(nano_ros REQUIRED)

# phase-263 Track C — per-host entry for `robot2`. `HOST robot2` keeps only the C++
# listener (`machine="robot2"`) from `multihost.launch.xml`, so this binary bakes the
# C++ half of the mixed topology.
nano_ros_add_executable(native_entry_robot2
    SOURCES src/main.cpp
    BOARD   native
    LAUNCH  "demo_bringup:multihost.launch.xml"
    TYPED
    HOST    robot2
    DEPLOY  native)
