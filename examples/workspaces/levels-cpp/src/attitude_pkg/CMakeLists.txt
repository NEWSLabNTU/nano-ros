cmake_minimum_required(VERSION 3.22)
project(attitude_pkg VERSION 0.1.0 LANGUAGES C CXX)

set(CMAKE_CXX_STANDARD 17)
set(CMAKE_CXX_STANDARD_REQUIRED ON)

find_package(nano_ros REQUIRED)
find_package(std_msgs REQUIRED)

# phase-162 W2 - attitude: one wall timer (1 ms, 1 kHz). CALLBACK_GROUPS
# declares the group a bringup binds or the derivation keys on.
nano_ros_auto_add_library(attitude_lib STATIC src/Attitude.cpp)

nros_components_register_node(attitude_lib
    HEADER attitude_pkg/Attitude.hpp
    PLUGIN attitude_pkg::Attitude
    EXECUTABLE attitude
    SHAPE rclcpp
    CALLBACK_GROUPS main)

if(TARGET std_msgs__nano_ros_cpp)
    target_link_libraries(attitude_lib
        PUBLIC std_msgs__nano_ros_cpp)
endif()
