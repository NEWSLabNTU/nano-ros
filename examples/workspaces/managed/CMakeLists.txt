cmake_minimum_required(VERSION 3.22)
project(ws_managed_cpp LANGUAGES C CXX)

find_package(nano_ros REQUIRED COMPONENTS workspace)

# phase-331 W2 — the MANUAL-transition (non-autostart) lifecycle demo. Split out
# of `features/` because that workspace's system declares
# `[lifecycle] autostart = "active"`, and one system cannot express both.
set(NANO_ROS_PLATFORM posix)

set(_ws_subdirs
    src/cpp_lifecycle_talker_pkg
    src/native_managed_entry)

# phase-348 W4 — the add_subdirectory() order comes from each package's
# `<depend>` tags. The list still chooses the SET (platform filtering is a
# selection no dependency can express); it no longer has to be written in a
# valid order.
nano_ros_workspace(
    ORDER_FROM_DEPENDS
    BACKEND  zenoh
    PLATFORM "${NANO_ROS_PLATFORM}"
    SYSTEM   demo_bringup
    SUBDIRS  ${_ws_subdirs})
