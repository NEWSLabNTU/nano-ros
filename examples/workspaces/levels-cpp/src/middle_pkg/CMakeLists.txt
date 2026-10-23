cmake_minimum_required(VERSION 3.22)
project(middle_pkg VERSION 0.1.0 LANGUAGES C CXX)

set(CMAKE_CXX_STANDARD 17)
set(CMAKE_CXX_STANDARD_REQUIRED ON)

find_package(nano_ros REQUIRED)
find_package(std_msgs REQUIRED)

# phase-509 P9 - middle: one wall timer (50 ms, 20 Hz). CALLBACK_GROUPS
# declares the one group the derivation keys on; the contract's rate is the
# only scheduling input, and each distinct rate is one preemption level.
nano_ros_auto_add_library(middle_lib STATIC src/Middle.cpp)

nros_components_register_node(middle_lib
    HEADER middle_pkg/Middle.hpp
    PLUGIN middle_pkg::Middle
    EXECUTABLE middle
    SHAPE rclcpp
    CALLBACK_GROUPS main)

if(TARGET std_msgs__nano_ros_cpp)
    target_link_libraries(middle_lib
        PUBLIC std_msgs__nano_ros_cpp)
endif()
