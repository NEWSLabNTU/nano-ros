cmake_minimum_required(VERSION 3.22)
project(seeds_cpp_pkg VERSION 0.1.0 LANGUAGES C CXX)

set(CMAKE_CXX_STANDARD 17)
set(CMAKE_CXX_STANDARD_REQUIRED ON)

find_package(nano_ros REQUIRED)

nano_ros_auto_add_library(seeds_cpp_lib STATIC src/Controller.cpp)
nros_components_register_node(seeds_cpp_lib
    HEADER seeds_cpp_pkg/Controller.hpp
    PLUGIN seeds_cpp_pkg::Controller
    EXECUTABLE seeds_cpp
    SHAPE rclcpp)
