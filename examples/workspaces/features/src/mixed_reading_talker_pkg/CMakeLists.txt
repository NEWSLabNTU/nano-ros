cmake_minimum_required(VERSION 3.22)
project(mixed_reading_talker_pkg VERSION 0.1.0 LANGUAGES C CXX)

set(CMAKE_C_STANDARD 11)
set(CMAKE_C_STANDARD_REQUIRED ON)

find_package(nano_ros REQUIRED)
find_package(custom_msgs REQUIRED)

# Typed component (RFC-0043) publishing the workspace-local
# `custom_msgs/Reading` through GENERATED C typesupport (phase-293 / issue
# #212 — struct + serde + type name come from the generated bindings).
nano_ros_auto_add_library(mixed_reading_talker_lib STATIC src/ReadingTalker.c)

nros_components_register_node(mixed_reading_talker_lib
    PLUGIN mixed_reading_talker_pkg::ReadingTalker
    EXECUTABLE mixed_reading_talker
    SHAPE configure
    TYPED)
