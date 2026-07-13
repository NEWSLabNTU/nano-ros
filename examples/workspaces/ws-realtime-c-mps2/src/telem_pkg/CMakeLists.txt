cmake_minimum_required(VERSION 3.22)
project(telem_pkg VERSION 0.1.0 LANGUAGES C CXX)

set(CMAKE_C_STANDARD 11)
set(CMAKE_C_STANDARD_REQUIRED ON)

find_package(nano_ros REQUIRED)
find_package(std_msgs REQUIRED)

# Generates the std_msgs C bindings (serialize/deserialize); the auto-link
# sidecar links the generated `std_msgs__nano_ros_c` interface lib.

# Low-tier telemetry node (C tiers fixture). Publishes a counter on /telem
# every 100 ms. CALLBACK_GROUPS declares the "telem" group ID; system.toml
# [[node_overrides]] maps it to the [tiers.low] tier (RFC-0015 §4.2).
nano_ros_add_node(telem CLASS telem_pkg::Telem TYPED CALLBACK_GROUPS telem SOURCES src/Telem.c)
