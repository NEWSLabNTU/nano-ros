cmake_minimum_required(VERSION 3.20)
project(consumer LANGUAGES CXX)

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

install(TARGETS consumer DESTINATION lib/${PROJECT_NAME})

ament_package()
