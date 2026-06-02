# Phase 212.M.10 fixture — Entry pkg (§212.L.3 replacement for the
# retired Bringup pkg). Native-only at the cmake surface (L.2). Links
# the talker_pkg + listener_pkg STATIC component libs registered by
# their respective `nano_ros_component_register(...)` calls.
cmake_minimum_required(VERSION 3.22)
project(demo_entry CXX)

nano_ros_entry(
    NAME demo_entry
    SOURCES src/main.cpp
    DEPLOY native)

target_link_libraries(demo_entry PRIVATE
    talker_pkg_talker_component
    listener_pkg_listener_component)
