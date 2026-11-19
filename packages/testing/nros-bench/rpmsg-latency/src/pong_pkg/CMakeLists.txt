cmake_minimum_required(VERSION 3.22)
project(pong_pkg VERSION 0.1.0 LANGUAGES C)

set(CMAKE_C_STANDARD 11)
set(CMAKE_C_STANDARD_REQUIRED ON)

find_package(nano_ros REQUIRED)
find_package(std_msgs REQUIRED)

nano_ros_auto_add_library(pong_lib STATIC src/Pong.c)

nros_components_register_node(pong_lib
    PLUGIN pong_pkg::Pong
    EXECUTABLE pong
    SHAPE configure
    TYPED)
