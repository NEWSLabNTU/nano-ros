cmake_minimum_required(VERSION 3.22)
project(reading_talker_pkg VERSION 0.1.0 LANGUAGES C CXX)

set(CMAKE_C_STANDARD 11)
set(CMAKE_C_STANDARD_REQUIRED ON)

find_package(nano_ros REQUIRED)
find_package(custom_msgs REQUIRED)

# Typed component (RFC-0043) publishing the workspace-local
# `custom_msgs/Reading` through GENERATED C typesupport (phase-293 / issue
# #212 — the raw hand-encoded-CDR shape is retired; struct + serialize come
# from the generated custom_msgs C bindings, same as std_msgs everywhere).
nano_ros_add_node(reading_talker CLASS reading_talker_pkg::ReadingTalker TYPED src/ReadingTalker.c)
