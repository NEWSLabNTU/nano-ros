cmake_minimum_required(VERSION 3.22)
project(gq_cpp_pkg VERSION 0.1.0 LANGUAGES C CXX)

set(CMAKE_CXX_STANDARD 17)
set(CMAKE_CXX_STANDARD_REQUIRED ON)

find_package(nano_ros REQUIRED)

nano_ros_auto_add_library(gq_cpp_lib STATIC src/Probe.cpp)
nros_components_register_node(gq_cpp_lib
    HEADER gq_cpp_pkg/Probe.hpp
    PLUGIN gq_cpp_pkg::Probe
    EXECUTABLE gq_cpp
    SHAPE rclcpp)
