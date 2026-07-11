cmake_minimum_required(VERSION 3.22)
project(baremetal_demo LANGUAGES C CXX)

set(CMAKE_C_STANDARD 11)
set(CMAKE_C_STANDARD_REQUIRED ON)
# --- nano-ros bootstrap (uniform across every example — do not hand-edit) ---
# Resolve the nano-ros checkout: -DNANO_ROS_ROOT → $NROS_REPO_DIR → in-tree
# walk-up. Everything after (helper include, per-pkg guard, RMW/CXX) is handled
# by nano_ros_bootstrap().
if(NOT DEFINED NANO_ROS_ROOT)
    if(NOT "$ENV{NROS_REPO_DIR}" STREQUAL "")
        set(NANO_ROS_ROOT "$ENV{NROS_REPO_DIR}")
    else()
        set(_nros_d "${CMAKE_CURRENT_SOURCE_DIR}")
        while(_nros_d AND NOT EXISTS "${_nros_d}/nros-sdk-index.toml")
            get_filename_component(_nros_d "${_nros_d}" DIRECTORY)
        endwhile()
        set(NANO_ROS_ROOT "${_nros_d}")
    endif()
endif()
include("${NANO_ROS_ROOT}/cmake/NanoRosBootstrap.cmake")
nano_ros_bootstrap()
# --- end nano-ros bootstrap ---
# Despite the "baremetal_demo" name, this example runs on the POSIX
# platform shim (the Rust port linked via `DEPLOY native`) so it builds
# and runs on a desktop. The lesson is twofold: the high-level nros C API
# in `src/main.c`, and `src/platform_impl.c` — a reference implementation
# of the canonical `<nros/platform.h>` ABI you would port to your target.
# Hard-codes zenoh; flipping RMWs is not the lesson here.
nano_ros_entry(
    NAME baremetal_demo
    SOURCES src/main.c
    DEPLOY native)

nros_platform_link_app(baremetal_demo)

# Compile-coverage for the platform-port reference. `src/platform_impl.c`
# defines the same `nros_platform_*` symbols the linked Rust POSIX port
# already provides, so it CANNOT be linked into `baremetal_demo` (duplicate
# definitions). Build it stand-alone instead — this keeps the reference in
# lockstep with the canonical header (`NanoRos::NanoRos` supplies the include
# path) without colliding. To use it as your actual platform, build nros with
# no Rust port and link this object into the firmware. See the file header.
add_library(baremetal_platform_ref STATIC src/platform_impl.c)
target_link_libraries(baremetal_platform_ref PRIVATE NanoRos::NanoRos)
target_compile_options(baremetal_platform_ref PRIVATE
    -Wall -Wextra -Wpedantic)

# Platform-specific libraries not covered by NanoRos::NanoRos.
if(UNIX)
    target_link_libraries(baremetal_demo PRIVATE rt)
endif()

target_compile_options(baremetal_demo PRIVATE
    -Wall -Wextra -Wpedantic
    -Wno-unused-result)
