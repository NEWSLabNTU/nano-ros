cmake_minimum_required(VERSION 3.22)
project(reading_talker_pkg VERSION 0.1.0 LANGUAGES C CXX)

set(CMAKE_CXX_STANDARD 14)
set(CMAKE_CXX_STANDARD_REQUIRED ON)

find_package(nano_ros REQUIRED)
find_package(custom_msgs REQUIRED)

# Typed component (RFC-0043) publishing the GENERATED
# `custom_msgs::msg::Reading` (phase-293 / issue #212 — the raw hand-encoded
# CDR shape is retired; the historical "double-builtin_interfaces glue" edge
# this pkg used to dodge died with the #203 idempotency/header-mirror repairs).
nano_ros_add_node(reading_talker CLASS reading_talker_pkg::ReadingTalker src/ReadingTalker.cpp)
