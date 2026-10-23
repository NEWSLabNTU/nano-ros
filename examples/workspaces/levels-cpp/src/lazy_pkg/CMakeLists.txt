cmake_minimum_required(VERSION 3.22)
project(lazy_pkg VERSION 0.1.0 LANGUAGES C CXX)

set(CMAKE_CXX_STANDARD 17)
set(CMAKE_CXX_STANDARD_REQUIRED ON)

find_package(nano_ros REQUIRED)
find_package(std_msgs REQUIRED)

# phase-509 P9 - lazy: one wall timer (200 ms, 5 Hz). CALLBACK_GROUPS
# declares the one group the derivation keys on; the contract's rate is the
# only scheduling input, and each distinct rate is one preemption level.
nano_ros_auto_add_library(lazy_lib STATIC src/Lazy.cpp)

nros_components_register_node(lazy_lib
    HEADER lazy_pkg/Lazy.hpp
    PLUGIN lazy_pkg::Lazy
    EXECUTABLE lazy
    SHAPE rclcpp
    CALLBACK_GROUPS main)

if(TARGET std_msgs__nano_ros_cpp)
    target_link_libraries(lazy_lib
        PUBLIC std_msgs__nano_ros_cpp)
endif()
