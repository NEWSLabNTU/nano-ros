cmake_minimum_required(VERSION 3.22)
project(subnode_pkg VERSION 0.1.0 LANGUAGES C CXX)

set(CMAKE_CXX_STANDARD 14)
set(CMAKE_CXX_STANDARD_REQUIRED ON)

find_package(nano_ros REQUIRED)
find_package(std_msgs REQUIRED)


# Sub-node realtime component (RFC-0047). ONE ComponentNode subclass that
# creates TWO callback groups: "ctrl" (10 ms) and "telem" (100 ms).
# CALLBACK_GROUPS declares both IDs; system.toml group_tiers assigns them to
# the deployment tiers — the package itself is tier-agnostic (portable).
nano_ros_add_node(subnode CLASS subnode_pkg::SubNode
    SHAPE rclcpp
    HEADER subnode_pkg/SubNode.hpp
    CALLBACK_GROUPS ctrl telem
    SOURCES src/SubNode.cpp)

# `nros_find_interfaces` declares INTERFACE libs (`std_msgs__nano_ros_cpp`)
# carrying generated headers' include dirs. Guard with `if(TARGET)`: some
# embedded generators fold the generated interface into the app target
# instead of exposing a `<pkg>__nano_ros_cpp` lib.
if(TARGET std_msgs__nano_ros_cpp)
    target_link_libraries(subnode_pkg_subnode_component
        PUBLIC std_msgs__nano_ros_cpp)
endif()
