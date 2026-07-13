cmake_minimum_required(VERSION 3.22)
project(threadx_entry VERSION 0.1.0 LANGUAGES C CXX)

set(CMAKE_CXX_STANDARD 14)
set(CMAKE_CXX_STANDARD_REQUIRED ON)

find_package(nano_ros REQUIRED)

# phase-263 C2c — the ThreadX-on-Linux embedded C++ workspace entry (the C++ sibling of
# C2a's C entry). The workspace root configured ThreadX
# (`-DNANO_ROS_PLATFORM=threadx -DNANO_ROS_BOARD=threadx-linux`). `BOARD threadx-linux`
# makes the codegen emit the ThreadX entry shape (`nros_app_main` +
# `ThreadxBoard::run_components`, issue 0097); `DEPLOY threadx-linux` matches
# `NANO_ROS_BOARD`, so `nano_ros_entry`'s embedded pass links the bootable image + bakes the
# locator. LAUNCH-only (no hand-written `main` — the generated TU carries `nros_app_main`).
nano_ros_add_executable(threadx_entry
    BOARD   threadx-linux
    LAUNCH  "demo_bringup:system.launch.xml"
    TYPED
    DEPLOY  threadx-linux)
