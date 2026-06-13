# Phase 212.H.4 fixture — minimal ThreadX adapter audit consumer.
#
# Exercises the codegen + Corrosion bridge surface from
# `cmake/NanoRosThreadxSystemCodegen.cmake` without dragging the full
# ThreadX kernel build in via `add_subdirectory(<nano-ros-root>)`.

cmake_minimum_required(VERSION 3.22)
project(threadx_app C)
set(CMAKE_EXPORT_COMPILE_COMMANDS ON)

# Corrosion is optional in this fixture — when missing the codegen helper
# only emits `system_main.c` + the `nros_system_main` static lib (no
# Rust component import). The test asserts both modes work.
find_package(Corrosion QUIET)

# Build-stage fixture (issue 0041): locate the repo's cmake module via a
# relative path (repo root = 6 levels up from this fixture dir:
# packages/testing/nros-tests/fixtures/multi_pkg_workspace_threadx/threadx_app)
# so the cmake-fixture mechanism can point `-S` straight at this dir with no
# `@NANO_ROS_ROOT@` rewrite — mirrors the `l9_register_*` fixtures.
get_filename_component(_NROS_ROOT
    "${CMAKE_CURRENT_SOURCE_DIR}/../../../../../.." ABSOLUTE)

# The fixture lives one directory below the bringup workspace root —
# WORKSPACE_ROOT points the helper at the parent (containing `src/`).
include("${_NROS_ROOT}/cmake/NanoRosThreadxSystemCodegen.cmake")
nros_threadx_codegen_system(
    SYSTEM demo_bringup
    WORKSPACE_ROOT "${CMAKE_CURRENT_SOURCE_DIR}/..")

add_executable(threadx_app main.c)
nros_threadx_link_app(threadx_app)
