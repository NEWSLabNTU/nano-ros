cmake_minimum_required(VERSION 3.22)
# §212.L.4 — class prefix must equal PROJECT_NAME.
project(clock_board_pkg VERSION 0.1.0 LANGUAGES C CXX)

set(CMAKE_CXX_STANDARD 17)
set(CMAKE_CXX_STANDARD_REQUIRED ON)

find_package(nano_ros REQUIRED)

# phase-582 W1d (RFC-0141 D2) -- the stub board of the `clock-board`
# configuration. The board hook is defined in the component's own TU, so it
# reaches exactly the images that launch this component.
nano_ros_auto_add_library(clock_board_lib STATIC src/ClockBoard.cpp)

nros_components_register_node(clock_board_lib
    PLUGIN clock_board_pkg::ClockBoard
    EXECUTABLE clock_board
    SHAPE configure
)
