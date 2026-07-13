cmake_minimum_required(VERSION 3.22)
# Phase 219.D — pure-C++ Entry pkg. The `LAUNCH …` keyword arg drives
# `nros codegen entry --lang cpp` at configure time, appends the
# generated `<NAME>_nros_main_generated.cpp` to the executable's
# sources, and `include()`s the sidecar `<NAME>_link_libs.cmake` so
# the Entry exe auto-links every Node pkg the launch XML pulls in.
project(native_entry VERSION 0.1.0 LANGUAGES C CXX)

set(CMAKE_CXX_STANDARD 14)
set(CMAKE_CXX_STANDARD_REQUIRED ON)

# Phase 219.I — workspace builds inherit the helpers via the parent
# root; standalone solo builds bootstrap via the include below.
find_package(nano_ros REQUIRED)

# Phase 247 (RFC-0043) — TYPED Entry. `nano_ros_entry(TYPED)` shells
# `nros codegen entry --lang cpp --typed`, routing each launch node to the real
# executor via its component object (construct + `configure(node)`) instead of
# the legacy `__nros_component_<pkg>_register` interpreter call.
nano_ros_add_executable(native_entry
    SOURCES src/main.cpp
    BOARD   native
    LAUNCH  "demo_bringup:system.launch.xml"
    TYPED
    DEPLOY  native)
