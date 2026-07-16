cmake_minimum_required(VERSION 3.22)
project(reading_listener_pkg VERSION 0.1.0 LANGUAGES C CXX)

set(CMAKE_C_STANDARD 11)
set(CMAKE_C_STANDARD_REQUIRED ON)

find_package(nano_ros REQUIRED)
find_package(custom_msgs REQUIRED)

# Typed component (RFC-0043) subscribing to the workspace-local
# `custom_msgs/Reading` through GENERATED C typesupport (phase-293 / issue
# #212 — struct + serde + type name come from the generated bindings).
nano_ros_add_node(reading_listener CLASS reading_listener_pkg::ReadingListener
    TYPED
    src/ReadingListener.c)
