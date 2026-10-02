# Issue 1620 — the configure-VERDICT fixture behind
# `cmake_platform_matrix::cmake_platform_threadx_requires_board`.
#
# `NANO_ROS_PLATFORM=threadx` without `NANO_ROS_BOARD` must FATAL_ERROR at
# configure time, naming NANO_ROS_BOARD (`threadx` is the one platform value
# that still needs a board to pick its std-vs-no_std split). The build stage
# configures this (`builder = "cmake-configure-verdict"`) and records the
# verdict; the test asserts it. It used to be generated and configured by the
# test itself.
cmake_minimum_required(VERSION 3.22)
project(plat_smoke C)

set(NANO_ROS_PLATFORM threadx)
set(NANO_ROS_RMW     zenoh)

add_subdirectory("@NANO_ROS_ROOT@" nano_ros)

add_executable(plat_smoke main.c)
target_link_libraries(plat_smoke PRIVATE NanoRos::NanoRos)
nros_platform_link_app(plat_smoke)
