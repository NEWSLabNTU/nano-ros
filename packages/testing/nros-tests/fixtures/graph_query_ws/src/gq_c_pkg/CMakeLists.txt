cmake_minimum_required(VERSION 3.22)
project(gq_c_pkg VERSION 0.1.0 LANGUAGES C)

set(CMAKE_C_STANDARD 11)
set(CMAKE_C_STANDARD_REQUIRED ON)

find_package(nano_ros REQUIRED)

nano_ros_auto_add_library(gq_c_lib STATIC src/Probe.c)
nros_components_register_node(gq_c_lib
    PLUGIN gq_c_pkg::Probe
    EXECUTABLE gq_c
    SHAPE configure
    TYPED)
