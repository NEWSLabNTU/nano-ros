# The FreeRTOS POSIX-simulator build of the SAME ported node.
#
# `../src/minimal_publisher.cpp` is the upstream ROS 2 tutorial, unmodified:
# it includes `<rclcpp/rclcpp.hpp>` and nothing of the board's entry headers,
# so no TU of this image includes `<rclc/detail/main.h>`. The image must link
# anyway: the board runner's optional hooks are weak references, not plain
# externs whose only definition is header-emitted (issue 2761).
#
# No `system.toml` here (a new one beside a build file is the retired leaf
# shape) and no platform word in this file (a node package is the same on
# every platform): the fixture row names the image at configure time,
# `-DNROS_PLATFORM=freertos -DNROS_BOARD=freertos-posix -DNROS_DEPLOY=freertos-posix`.
cmake_minimum_required(VERSION 3.22)
project(minimal_publisher LANGUAGES C CXX)
set(CMAKE_CXX_STANDARD 17)
set(CMAKE_CXX_STANDARD_REQUIRED ON)

find_package(nano_ros REQUIRED)
find_package(std_msgs REQUIRED)

nano_ros_add_executable(minimal_publisher ../src/minimal_publisher.cpp ROS2_MAIN)
ament_target_dependencies(minimal_publisher std_msgs)
