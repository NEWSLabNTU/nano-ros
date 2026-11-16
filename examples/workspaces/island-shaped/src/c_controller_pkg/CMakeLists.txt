cmake_minimum_required(VERSION 3.22)
project(c_controller_pkg VERSION 0.1.0 LANGUAGES C)

set(CMAKE_C_STANDARD 11)
set(CMAKE_C_STANDARD_REQUIRED ON)

find_package(nano_ros REQUIRED)
find_package(std_msgs REQUIRED)

# Typed component (RFC-0043): two subscriptions of two types, two publishers
# and a control timer. The param files under config/ are not compiled; the
# bringup's launch file loads them with <param from>.
nano_ros_auto_add_library(controller_lib STATIC src/Controller.c)

nros_components_register_node(controller_lib
    PLUGIN c_controller_pkg::Controller
    EXECUTABLE controller
    SHAPE configure
    TYPED)
