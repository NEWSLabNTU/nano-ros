cmake_minimum_required(VERSION 3.22)
project(native_threadx_entry VERSION 0.1.0 LANGUAGES C CXX)

set(CMAKE_C_STANDARD 11)
set(CMAKE_C_STANDARD_REQUIRED ON)

find_package(nano_ros REQUIRED)

# phase-263 C2a — the FIRST C/C++ workspace EMBEDDED entry: ThreadX-on-Linux (host x86
# sim — builds AND runs natively, no QEMU). The workspace root configured ThreadX
# (`-DNANO_ROS_PLATFORM=threadx -DNANO_ROS_BOARD=threadx-linux`), so the platform+board
# overlays are loaded. `BOARD threadx-linux` makes the codegen emit the ThreadX entry
# shape (`nros_app_main` + `ThreadxBoard::run_components`, issue 0097); `DEPLOY
# threadx-linux` matches `NANO_ROS_BOARD`, so `nano_ros_entry`'s embedded link pass calls
# `nros_platform_link_app` (startup + app_define + kernel + nsos-netx + RMW). LAUNCH-only:
# the generated `.cpp` TU carries the entry; no hand-written `main`.
nano_ros_add_executable(native_threadx_entry
    BOARD   threadx-linux
    LAUNCH  "demo_bringup:system.launch.xml"
    LANG    c
    TYPED
    DEPLOY  threadx-linux)
