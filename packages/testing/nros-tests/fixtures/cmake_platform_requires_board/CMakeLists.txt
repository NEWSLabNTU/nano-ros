# Issue 1620 — the configure-VERDICT fixture behind
# `cmake_platform_matrix::cmake_platform_threadx_requires_board`.
#
# `NROS_PLATFORM=threadx` without `NROS_BOARD` must FATAL_ERROR at
# configure time, naming NROS_BOARD (`threadx` is the one platform value
# that still needs a board to pick its std-vs-no_std split). The build stage
# configures this (`builder = "cmake-configure-verdict"`) and records the
# verdict; the test asserts it. It used to be generated and configured by the
# test itself.
cmake_minimum_required(VERSION 3.22)
project(plat_smoke C)

set(NROS_PLATFORM threadx)
set(NROS_RMW     zenoh)

# The FATAL_ERROR fires inside this call, so nothing below it would run. No
# executable is declared on purpose: an image here would have to claim a panic
# ending (`check-cmake-image-policy`) for a link that must never happen.
add_subdirectory("@NROS_REPO_DIR@" nano_ros)
