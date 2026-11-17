# Issue 2680 -- a plain library package: no node, no nano_ros. A node package
# that <depend>s on it links the target, and expects the workspace (and the
# metadata probe) to have added this package first.
cmake_minimum_required(VERSION 3.22)
project(period_lib LANGUAGES C)

add_library(period_lib INTERFACE)
target_include_directories(period_lib INTERFACE "${CMAKE_CURRENT_SOURCE_DIR}/include")
