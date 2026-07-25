cmake_minimum_required(VERSION 3.22)
project(telem_pkg VERSION 0.1.0 LANGUAGES C CXX)

set(CMAKE_CXX_STANDARD 14)
set(CMAKE_CXX_STANDARD_REQUIRED ON)

find_package(nano_ros REQUIRED)
find_package(std_msgs REQUIRED)


# rclcpp-shape low-tier telemetry node (RFC-0047, issue #124). Publishes a
# counter on /telem every 100 ms as an IS-A-node (nros::ComponentNode
# subclass). SHAPE rclcpp; tier binding via bind_node_name_sched.
nano_ros_auto_add_library(telem_lib STATIC src/Telem.cpp)

nros_components_register_node(telem_lib
    HEADER telem_pkg/Telem.hpp
    PLUGIN telem_pkg::Telem
    EXECUTABLE telem
    SHAPE rclcpp
    CALLBACK_GROUPS telem)

# `nros_find_interfaces` declares INTERFACE libs (`std_msgs__nano_ros_cpp`)
# carrying generated headers' include dirs. Guard with `if(TARGET)`: some
# embedded generators fold the generated interface into the app target
# instead of exposing a `<pkg>__nano_ros_cpp` lib.
if(TARGET std_msgs__nano_ros_cpp)
    target_link_libraries(telem_lib
        PUBLIC std_msgs__nano_ros_cpp)
endif()
