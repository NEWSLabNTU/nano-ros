cmake_minimum_required(VERSION 3.22)
project(ctrl_pkg VERSION 0.1.0 LANGUAGES C CXX)

set(CMAKE_CXX_STANDARD 14)
set(CMAKE_CXX_STANDARD_REQUIRED ON)

find_package(nano_ros REQUIRED)
find_package(std_msgs REQUIRED)


# rclcpp-shape high-tier control node (RFC-0047, issue #124). Publishes a
# counter on /ctrl every 10 ms as an IS-A-node (nros::ComponentNode subclass).
# SHAPE rclcpp tells the codegen to emit placement-new with executor handle;
# the tier binding is seeded via bind_node_name_sched (not NodeBuilder::sched).
nano_ros_auto_add_library(ctrl_lib STATIC src/Ctrl.cpp)

nros_components_register_node(ctrl_lib
    HEADER ctrl_pkg/Ctrl.hpp
    PLUGIN ctrl_pkg::Ctrl
    EXECUTABLE ctrl
    SHAPE rclcpp
    CALLBACK_GROUPS ctrl)

# `nros_find_interfaces` declares INTERFACE libs (`std_msgs__nano_ros_cpp`)
# carrying generated headers' include dirs. Guard with `if(TARGET)`: some
# embedded generators fold the generated interface into the app target
# instead of exposing a `<pkg>__nano_ros_cpp` lib.
if(TARGET std_msgs__nano_ros_cpp)
    target_link_libraries(ctrl_lib
        PUBLIC std_msgs__nano_ros_cpp)
endif()
