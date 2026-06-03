# Phase 212.M.10 fixture — Component pkg under §212.L taxonomy. The
# `project()` call exists so the L.4 CLASS prefix rule
# (`CLASS startswith ${PROJECT_NAME}::`) resolves to `talker_pkg::`.
cmake_minimum_required(VERSION 3.22)
project(talker_pkg CXX)

nano_ros_node_register(
    NAME talker
    CLASS talker_pkg::Talker
    SOURCES src/talker.cpp
    DEPLOY native)
