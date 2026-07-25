# Phase 212.M.10 fixture — Component pkg under §212.L taxonomy. The
# `project()` call exists so the L.4 CLASS prefix rule
# (`CLASS startswith ${PROJECT_NAME}::`) resolves to `listener_pkg::`.
cmake_minimum_required(VERSION 3.22)
project(listener_pkg CXX)

nano_ros_auto_add_library(listener_lib STATIC src/listener.cpp)

nros_components_register_node(listener_lib
    PLUGIN listener_pkg::Listener
    EXECUTABLE listener
    SHAPE configure
    DEPLOY native)
