cmake_minimum_required(VERSION 3.22)
# `nano_ros_application()` + the codegen target_link_libraries chain
# pull in C++ pieces (cbindgen-generated FFI glue, Cyclone optional
# branch); enable both languages so the cmake fns don't trip on
# missing CXX feature detection.
project(c_talker LANGUAGES C CXX)

set(CMAKE_C_STANDARD 11)
set(CMAKE_C_STANDARD_REQUIRED ON)

# Phase 212.M (native/c sweep, follows §212.M.2 cpp pattern) —
# Application pkg shape, C variant.
#   * `package.xml`            — pkg identity + msg dep declaration
#   * `nros_find_interfaces`   — reads package.xml, generates the
#                                C-side msg bindings closure
#   * `nano_ros_application()` — declares the exec entity + appends
#                                metadata for `nros codegen-system`
#
# RMW selected at cmake configure time via `-DNROS_RMW=<rmw>` (default
# zenoh). `NANO_ROS_PLATFORM` / `NANO_ROS_RMW` are still set here (the
# root CMake reads them at `add_subdirectory()` time).
set(NANO_ROS_PLATFORM posix)
set(NROS_RMW "zenoh" CACHE STRING
    "Active RMW (zenoh|cyclonedds|xrce) — selects the backend linked into c_talker.")
set(NANO_ROS_RMW "${NROS_RMW}")

# The Cyclone DDS backend is C++ (operator new/delete, std::nothrow). A C app
# linking it gets LINKER_LANGUAGE CXX (set by nano_ros_link_rmw), which needs
# CMAKE_CXX_LINK_EXECUTABLE — i.e. CXX enabled in *this* directory scope. The
# nano-ros add_subdirectory enables CXX only in its own subtree, so enable it
# here for the Cyclone RMW. (zenoh / xrce stay pure C.)
if(NROS_RMW STREQUAL "cyclonedds")
    enable_language(CXX)
endif()

set(_NANO_ROS_ROOT "${CMAKE_CURRENT_SOURCE_DIR}/../../../..")
add_subdirectory("${_NANO_ROS_ROOT}" nano_ros)

# `NanoRosNodeRegister.cmake` ships the §212.L.9 cmake fns
# (`nano_ros_application`, `nano_ros_component_register`,
# `nano_ros_deploy`). Per-example include matches the §212.M.2 pattern.
include("${_NANO_ROS_ROOT}/cmake/NanoRosNodeRegister.cmake")

# Phase 210.E.4 — `nros_find_interfaces` reads `package.xml` and
# generates C-side bindings (+ FFI glue) for every transitive interface
# dep. Replaces the per-`nros_generate_interfaces(<pkg>)` chain the
# pre-212 shape used.
nros_find_interfaces(LANGUAGE C SKIP_INSTALL)

nano_ros_application(
    NAME c_talker
    SOURCES src/main.c
    DEPLOY native)

target_link_libraries(c_talker PRIVATE std_msgs__nano_ros_c)
nros_platform_link_app(c_talker)
