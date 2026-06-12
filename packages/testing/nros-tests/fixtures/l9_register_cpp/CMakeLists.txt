cmake_minimum_required(VERSION 3.22)
project(talker_pkg C CXX)
# Build-stage fixture (issue 0041): include the repo's cmake module via a
# relative path (repo root = 5 levels up from this fixture dir).
get_filename_component(_NROS_ROOT "${CMAKE_CURRENT_SOURCE_DIR}/../../../../.." ABSOLUTE)
include("${_NROS_ROOT}/cmake/NanoRosNodeRegister.cmake")
nano_ros_node_register(NAME talker CLASS talker_pkg::Talker SOURCES src/dummy.cpp DEPLOY native zephyr)
