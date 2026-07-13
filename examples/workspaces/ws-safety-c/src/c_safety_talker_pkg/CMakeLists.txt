cmake_minimum_required(VERSION 3.22)
project(c_safety_talker_pkg VERSION 0.1.0 LANGUAGES C CXX)

set(CMAKE_C_STANDARD 11)
set(CMAKE_C_STANDARD_REQUIRED ON)

find_package(nano_ros REQUIRED)
find_package(std_msgs REQUIRED)

# Generates the std_msgs C bindings (serialize/deserialize); the auto-link
# sidecar links the generated `std_msgs__nano_ros_c` interface lib.

# Typed managed-node C component: publishes a counter on /chatter via the
# generated std_msgs/Int32 serializer. With NANO_ROS_SAFETY_E2E=ON (from
# [system].features = ["safety"]) the zenoh backend automatically attaches a
# CRC-32 + sequence number on every publish — no component code change
# required. The CRC is what the paired safe_listener validates.
nano_ros_add_node(talker CLASS c_safety_talker_pkg::Talker TYPED src/SafetyTalker.c)
