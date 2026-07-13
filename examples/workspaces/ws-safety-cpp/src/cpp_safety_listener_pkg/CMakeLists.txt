cmake_minimum_required(VERSION 3.22)
project(cpp_safety_listener_pkg VERSION 0.1.0 LANGUAGES C CXX)

set(CMAKE_CXX_STANDARD 14)
set(CMAKE_CXX_STANDARD_REQUIRED ON)

find_package(nano_ros REQUIRED)
find_package(std_msgs REQUIRED)


# Phase 269 W3 — C++ safety-listener component. Uses
# `node.create_subscription_with_safety<M>()` to register an integrity-carrying
# subscription on /chatter; the typed callback receives (const Int32&, const
# nros_cpp_integrity_status_t&). Requires NANO_ROS_SAFETY_E2E=ON.
nano_ros_add_node(safe_listener CLASS cpp_safety_listener_pkg::SafetyListener
    src/SafetyListener.cpp)

if(TARGET std_msgs__nano_ros_cpp)
    target_link_libraries(cpp_safety_listener_pkg_safe_listener_component
        PUBLIC std_msgs__nano_ros_cpp)
endif()
