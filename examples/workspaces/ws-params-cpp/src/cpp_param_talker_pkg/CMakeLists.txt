cmake_minimum_required(VERSION 3.22)
project(cpp_param_talker_pkg VERSION 0.1.0 LANGUAGES C CXX)

set(CMAKE_CXX_STANDARD 14)
set(CMAKE_CXX_STANDARD_REQUIRED ON)

find_package(nano_ros REQUIRED)
find_package(std_msgs REQUIRED)


# Phase 269 W1 — typed C++ component that reads publish_period_ms live each tick
# via nros_cpp_get_param_integer on the saved executor handle.
nano_ros_add_node(param_talker CLASS cpp_param_talker_pkg::ParamTalker src/ParamTalker.cpp)

if(TARGET std_msgs__nano_ros_cpp)
    target_link_libraries(cpp_param_talker_pkg_param_talker_component
        PUBLIC std_msgs__nano_ros_cpp)
endif()
