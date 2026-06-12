cmake_minimum_required(VERSION 3.22)
project(component_node_poc LANGUAGES C CXX)

set(CMAKE_CXX_STANDARD 14)
set(CMAKE_CXX_STANDARD_REQUIRED ON)

# Phase 242.1 + 242.2 (RFC-0044) — proof that a node deriving nros::ComponentNode
# (IS-A node, ctor-creates a publisher + a typed member-callback subscription)
# runs through the real executor and exchanges a typed message. Mirrors the
# RFC-0043 component-poc one dir over, but on the rclcpp-faithful base.
if(NOT COMMAND nano_ros_workspace_pkg_guard)
    include("${CMAKE_CURRENT_SOURCE_DIR}/../../../../cmake/NanoRosWorkspace.cmake")
endif()
nano_ros_workspace_pkg_guard()
nros_find_interfaces(LANGUAGE CPP SKIP_INSTALL)

# No LAUNCH arg → nano_ros_entry uses the hand-written main.cpp as the entry
# (imperative, no codegen path).
nano_ros_entry(
    NAME component_node_poc
    SOURCES src/main.cpp
    DEPLOY native)

target_link_libraries(component_node_poc PRIVATE std_msgs__nano_ros_cpp)
nros_platform_link_app(component_node_poc)
