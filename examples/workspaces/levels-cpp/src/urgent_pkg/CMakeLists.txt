cmake_minimum_required(VERSION 3.22)
project(urgent_pkg VERSION 0.1.0 LANGUAGES C CXX)

set(CMAKE_CXX_STANDARD 17)
set(CMAKE_CXX_STANDARD_REQUIRED ON)

find_package(nano_ros REQUIRED)
find_package(std_msgs REQUIRED)

# phase-509 P9 - urgent: one wall timer (20 ms, 50 Hz). CALLBACK_GROUPS
# declares the one group the derivation keys on; the contract's rate is the
# only scheduling input, and each distinct rate is one preemption level.
nano_ros_auto_add_library(urgent_lib STATIC src/Urgent.cpp)

nros_components_register_node(urgent_lib
    HEADER urgent_pkg/Urgent.hpp
    PLUGIN urgent_pkg::Urgent
    EXECUTABLE urgent
    SHAPE rclcpp
    CALLBACK_GROUPS main)

if(TARGET std_msgs__nano_ros_cpp)
    target_link_libraries(urgent_lib
        PUBLIC std_msgs__nano_ros_cpp)
endif()
