cmake_minimum_required(VERSION 3.22)
project(safety_workspace LANGUAGES C CXX)

find_package(nano_ros REQUIRED COMPONENTS workspace)

# phase-331 W4 (RFC-0066) — the E2E-safety demo, all three languages.
#
# ONE workspace rather than three (`ws-safety-{c,cpp,rust}`), and NOT folded
# into `features/`: this system declares `features = ["safety"]`, and a system
# carries exactly one feature set. The validated listener has nothing to
# validate unless safety-e2e is on, so it cannot share `features/`'s system.
#
# `safety-e2e` changes probed ABI sizes, which is why the fixture rows give this
# workspace its own build dir rather than sharing one with a non-safety build
# (the sizes-mirror guard would otherwise fire).
set(NANO_ROS_PLATFORM posix)

set(_ws_subdirs
    src/c_safety_talker_pkg
    src/c_safety_listener_pkg
    src/cpp_safety_talker_pkg
    src/cpp_safety_listener_pkg
    src/native_c_safety_talker_entry
    src/native_c_safety_listener_entry
    src/native_cpp_safety_talker_entry
    src/native_cpp_safety_listener_entry)

nano_ros_workspace(
    BACKEND  zenoh
    PLATFORM "${NANO_ROS_PLATFORM}"
    SYSTEM   demo_bringup
    SUBDIRS  ${_ws_subdirs})
