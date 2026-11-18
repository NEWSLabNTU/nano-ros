cmake_minimum_required(VERSION 3.22)
project(seeds_c_pkg VERSION 0.1.0 LANGUAGES C)

set(CMAKE_C_STANDARD 11)
set(CMAKE_C_STANDARD_REQUIRED ON)

find_package(nano_ros REQUIRED)

nano_ros_auto_add_library(seeds_c_lib STATIC src/Controller.c)
nros_components_register_node(seeds_c_lib
    PLUGIN seeds_c_pkg::Controller
    EXECUTABLE seeds_c
    SHAPE configure
    TYPED)
