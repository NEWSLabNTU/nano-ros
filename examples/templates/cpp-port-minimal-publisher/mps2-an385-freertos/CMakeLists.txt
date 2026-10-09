# The FreeRTOS (mps2-an385, QEMU) build of the SAME ported node — phase-482 W3.
#
# `../src/minimal_publisher.cpp` is the upstream ROS 2 tutorial, unmodified,
# `int main(int argc, char** argv)` included. This file is the whole port: the
# board comes from `system.toml` beside it, and `ROS2_MAIN` tells nano-ros the
# sources are a ROS 2 program whose `main` must be reached through the board's
# startup (`nros_app_main`), not called by a C runtime that does not exist here.
cmake_minimum_required(VERSION 3.22)
project(minimal_publisher LANGUAGES C CXX)
set(CMAKE_CXX_STANDARD 17)
set(CMAKE_CXX_STANDARD_REQUIRED ON)

find_package(nano_ros REQUIRED)
find_package(std_msgs REQUIRED)

nano_ros_add_executable(minimal_publisher ../src/minimal_publisher.cpp ROS2_MAIN)
ament_target_dependencies(minimal_publisher std_msgs)
