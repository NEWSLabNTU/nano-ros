cmake_minimum_required(VERSION 3.22)
project(threadx_entry VERSION 0.1.0 LANGUAGES C CXX)

set(CMAKE_CXX_STANDARD 14)
set(CMAKE_CXX_STANDARD_REQUIRED ON)

find_package(nano_ros REQUIRED)

# phase-263 C2c — the ThreadX-on-Linux embedded entry for the MIXED workspace: a C talker, a
# C++ listener, AND a Rust heartbeat node combined into one bootable image. The Rust node is
# linked via the `nros_ws_runtime` umbrella (re-points NanoRos::NanoRosCpp); on threadx-linux
# the cargo target is the host x86_64 triple (the ThreadX simulation port runs as pthreads),
# so the Rust node compiles host-side like the native mixed entry. `BOARD threadx-linux` →
# the codegen emits the ThreadX entry shape (`nros_app_main` + `ThreadxBoard::run_components`,
# issue 0097); the embedded pass links the image + bakes the locator. LAUNCH-only.
nano_ros_add_executable(threadx_entry
    BOARD   threadx-linux
    LAUNCH  "demo_bringup:system.launch.xml"
    TYPED
    DEPLOY  threadx-linux)
