cmake_minimum_required(VERSION 3.22)
project(load_pkg VERSION 0.1.0 LANGUAGES C CXX)

set(CMAKE_CXX_STANDARD 17)
set(CMAKE_CXX_STANDARD_REQUIRED ON)

find_package(nano_ros REQUIRED)
find_package(std_msgs REQUIRED)

# phase-162 W1/W2 - load: one wall timer (100 ms, 10 Hz). CALLBACK_GROUPS
# declares the group a bringup binds or the derivation keys on.
nano_ros_auto_add_library(load_lib STATIC src/Load.cpp)

nros_components_register_node(load_lib
    HEADER load_pkg/Load.hpp
    PLUGIN load_pkg::Load
    EXECUTABLE load
    SHAPE rclcpp
    CALLBACK_GROUPS main)

if(TARGET std_msgs__nano_ros_cpp)
    target_link_libraries(load_lib
        PUBLIC std_msgs__nano_ros_cpp)
endif()
