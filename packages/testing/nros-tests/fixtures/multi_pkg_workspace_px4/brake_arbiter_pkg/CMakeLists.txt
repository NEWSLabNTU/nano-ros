# Phase 212.M.10 fixture — brake_arbiter component pkg (PX4 host-compile shape).
#
# Per §212.L.9 the C++ pkg metadata surface is the cmake fn pair
# `nano_ros_component_register` + `nano_ros_deploy` — sidecar
# `component_nros.toml` is forbidden (§Non-Goals + §212.M.10).

cmake_minimum_required(VERSION 3.22)
# §212.L.4 — CLASS prefix must equal PROJECT_NAME (pkg dir name).
project(brake_arbiter_pkg VERSION 0.1.0 LANGUAGES CXX)

set(CMAKE_CXX_STANDARD 14)
set(CMAKE_CXX_STANDARD_REQUIRED ON)

include("${CMAKE_CURRENT_SOURCE_DIR}/../../../../../../cmake/NanoRosComponentRegister.cmake")

nano_ros_component_register(
    NAME    brake_arbiter
    CLASS   brake_arbiter_pkg::brake_arbiter
    SOURCES src/brake_arbiter.cpp
    DEPLOY  px4)

nano_ros_deploy(
    TARGET    px4
    RMW       cyclonedds
    DOMAIN_ID 0)
