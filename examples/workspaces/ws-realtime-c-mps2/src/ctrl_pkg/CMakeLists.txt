cmake_minimum_required(VERSION 3.22)
project(ctrl_pkg VERSION 0.1.0 LANGUAGES C CXX)

set(CMAKE_C_STANDARD 11)
set(CMAKE_C_STANDARD_REQUIRED ON)

find_package(nano_ros REQUIRED)
find_package(std_msgs REQUIRED)

# Generates the std_msgs C bindings (serialize/deserialize); the auto-link
# sidecar links the generated `std_msgs__nano_ros_c` interface lib.

# High-tier control node (C tiers fixture). Publishes a counter on /ctrl
# every 10 ms. CALLBACK_GROUPS declares the "ctrl" group ID; system.toml
# [[node_overrides]] maps it to the [tiers.high] tier (RFC-0015 §4.2).
nano_ros_add_node(ctrl CLASS ctrl_pkg::Ctrl TYPED CALLBACK_GROUPS ctrl SOURCES src/Ctrl.c)
