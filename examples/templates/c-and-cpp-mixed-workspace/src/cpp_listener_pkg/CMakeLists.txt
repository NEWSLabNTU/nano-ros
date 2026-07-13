cmake_minimum_required(VERSION 3.22)
project(cpp_listener_pkg VERSION 0.1.0 LANGUAGES C CXX)

set(CMAKE_CXX_STANDARD 14)
set(CMAKE_CXX_STANDARD_REQUIRED ON)

find_package(nano_ros REQUIRED)
find_package(std_msgs REQUIRED)


nano_ros_add_node(listener CLASS cpp_listener_pkg::Listener src/Listener.cpp)

target_link_libraries(cpp_listener_pkg_listener_component
    PUBLIC std_msgs__nano_ros_cpp)
