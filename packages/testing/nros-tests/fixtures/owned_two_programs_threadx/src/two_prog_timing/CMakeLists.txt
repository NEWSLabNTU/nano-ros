# Issue 2899 -- a plain library package the owned entry `two_prog` <depend>s
# on: no node, no nano_ros. `two_prog` includes its header and does NOT add
# this package itself, so the owned entry's census build has to add it, as
# the workspace build and the metadata probe do.
cmake_minimum_required(VERSION 3.22)
project(two_prog_timing LANGUAGES C)

add_library(two_prog_timing INTERFACE)
target_include_directories(two_prog_timing INTERFACE "${CMAKE_CURRENT_SOURCE_DIR}/include")
