cmake_minimum_required(VERSION 3.22)
project(ping_pkg VERSION 0.1.0 LANGUAGES C)

set(CMAKE_C_STANDARD 11)
set(CMAKE_C_STANDARD_REQUIRED ON)

find_package(nano_ros REQUIRED)
find_package(std_msgs REQUIRED)

nano_ros_auto_add_library(ping_lib STATIC src/Ping.c)

nros_components_register_node(ping_lib
    PLUGIN ping_pkg::Ping
    EXECUTABLE ping
    SHAPE configure
    TYPED)
