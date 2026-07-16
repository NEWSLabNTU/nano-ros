cmake_minimum_required(VERSION 3.22)
project(reading_listener_pkg VERSION 0.1.0 LANGUAGES C CXX)

set(CMAKE_CXX_STANDARD 14)
set(CMAKE_CXX_STANDARD_REQUIRED ON)

find_package(nano_ros REQUIRED)
find_package(custom_msgs REQUIRED)

# Typed component (RFC-0043) subscribing through the GENERATED
# `custom_msgs::msg::Reading` bindings (phase-293 / issue #212).
nano_ros_add_node(reading_listener CLASS reading_listener_pkg::ReadingListener
    src/ReadingListener.cpp)
