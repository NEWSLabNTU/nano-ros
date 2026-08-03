cmake_minimum_required(VERSION 3.22)
project(native_entry_robot1 VERSION 0.1.0 LANGUAGES C CXX)

set(CMAKE_CXX_STANDARD 14)
set(CMAKE_CXX_STANDARD_REQUIRED ON)

find_package(nano_ros REQUIRED)

# phase-326 (issue 0364) — per-host entry for `robot1`. The partition happens at
# RESOLVE time: `multihost_robot1_model.yaml` (resolved with `host:=robot1`)
# contains only the C talker + Rust heartbeat, so this binary bakes that host's slice of the
# multi-host topology. (The retired HOST keyword partitioned at bake time from
# the ROS 1-ism `<node machine=…>`.)
nano_ros_add_executable(native_entry_robot1
    SOURCES src/main.cpp
    BOARD   native
    BRINGUP "${CMAKE_CURRENT_SOURCE_DIR}/../demo_bringup"
    LAUNCH  multihost.launch.xml
    LAUNCH_ARGS host=robot1
    TYPED
    DEPLOY  native)
