cmake_minimum_required(VERSION 3.22)
project(clock_board_pkg VERSION 0.1.0 LANGUAGES C)

set(CMAKE_C_STANDARD 11)
set(CMAKE_C_STANDARD_REQUIRED ON)

find_package(nano_ros REQUIRED)

# The stub board of the `clock-board` configuration, in C (issue 2841; the C++
# workspace's `clock_board_pkg` is its twin). The board hook is defined in the
# component's own TU, so it reaches exactly the images that launch this
# component.
nano_ros_auto_add_library(clock_board_lib STATIC src/ClockBoard.c)

nros_components_register_node(clock_board_lib
    PLUGIN clock_board_pkg::ClockBoard
    EXECUTABLE clock_board
    SHAPE configure
    TYPED)
