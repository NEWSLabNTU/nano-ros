cmake_minimum_required(VERSION 3.22)
project(multi_node_workspace_cpp LANGUAGES C CXX)

# Phase 219.I — workspace-root single call. Replaces the per-subdir
# `add_subdirectory(<nano-ros>)` + `include(NanoRosNodeRegister.cmake)`
# boilerplate the pre-219 multi-pkg examples carried; each subdir below
# uses `nano_ros_workspace_pkg_guard()` to inherit the helpers without
# re-importing nano-ros (the cmake equivalent of cargo `[workspace]`
# discipline).
#
# Standalone solo build of any subdir:
#     cmake -S src/talker_pkg -B build -DNANO_ROS_ROOT=/path/to/nano-ros
# Workspace build (from this dir):
#     cmake -S . -B build -DNANO_ROS_ROOT=/path/to/nano-ros && cmake --build build
include("${CMAKE_CURRENT_SOURCE_DIR}/../../../cmake/NanoRosWorkspace.cmake")

nano_ros_workspace(
    BACKEND  zenoh
    PLATFORM posix
    NANO_ROS_ROOT "${CMAKE_CURRENT_SOURCE_DIR}/../../.."
    SUBDIRS  src/talker_pkg
             src/listener_pkg
             src/native_entry
    # Phase 219.D — `src/native_entry/CMakeLists.txt` calls
    # `nano_ros_entry(LAUNCH "demo_bringup:system.launch.xml")`. The
    # cmake fn shells `nros codegen entry --lang cpp` at configure
    # time, emits the canonical `int main()` body into the build dir,
    # and auto-links the matching Node-pkg static libs. Their CMake
    # target names still end in `_component` for ABI compatibility.
)
