cmake_minimum_required(VERSION 3.20)
# C enabled alongside CXX: `nano_ros_link_rmw(consumer)` below wires the
# RMW-backend registration stub, a generated `.c` (this exe links nano-ros's cpp
# std_msgs binding → libnros_cpp.a → that stub). nros-cpp itself is C++.
project(consumer LANGUAGES C CXX)

find_package(ament_cmake REQUIRED)
find_package(rclcpp REQUIRED)

# `std_msgs` resolves through the smart Find-stub:
#   Layer 1: NROS_INTERFACE_SEARCH_PATH = ../std_msgs  (THIS pkg)
#   Layer 2: AMENT_PREFIX_PATH          = /opt/ros/<distro>/share/std_msgs
#   Layer 3: bundled                    = <nano-ros>/packages/interfaces/...
# Layer 1 wins and emits a `message(STATUS nros: find_package(std_msgs)
# -> <workspace>/src/std_msgs)` line.
find_package(std_msgs REQUIRED)

add_executable(consumer src/consumer.cpp)

ament_target_dependencies(consumer
    rclcpp
    std_msgs
)

# nano-ros backend registration: nros-cpp's runtime references
# `nros_app_register_backends()`, whose sole (strong) definition is emitted by
# `nano_ros_link_rmw` (phase-249 P4a removed the weak default, so its absence is
# a link error). The workspace's hand-written umbrella used to wire this; with
# no root build file (RFC-0098 D9, phase-445 W5) the package states it itself,
# like `local-msg-package/src/consumer`. A no-op under stock ROS — the compat
# module is what defines the command.
if(COMMAND nano_ros_link_rmw)
    nano_ros_link_rmw(consumer)
endif()

install(TARGETS consumer DESTINATION lib/${PROJECT_NAME})

ament_package()
