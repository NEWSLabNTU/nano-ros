cmake_minimum_required(VERSION 3.22)
project(cyclonedds_cpp_action_client VERSION 0.1.0 LANGUAGES CXX C)

set(CMAKE_CXX_STANDARD 14)
set(CMAKE_CXX_STANDARD_REQUIRED ON)

# Phase 144.4 — consume nano-ros via add_subdirectory(<repo-root>).
set(NANO_ROS_PLATFORM posix)
set(NANO_ROS_RMW     cyclonedds)
set(NROS_RMW_CYCLONEDDS_MSG_TO_IDL
    "${CMAKE_CURRENT_SOURCE_DIR}/../../../../../scripts/cyclonedds/msg_to_cyclone_idl.py"
    CACHE FILEPATH "nano-ros Cyclone DDS .msg/.srv to IDL converter" FORCE)
add_subdirectory("${CMAKE_CURRENT_SOURCE_DIR}/../../../../.." nano_ros)

# Actions need the action infrastructure types' Cyclone descriptors
# (cancel_goal → action_msgs/CancelGoal, status → GoalStatusArray,
# transitively unique_identifier_msgs/UUID + builtin_interfaces). The
# cpp FFI `include!()` of a dependency's `.rs` is NOT transitive, so
# `example_interfaces` lists the full transitive closure.
nros_generate_interfaces(builtin_interfaces LANGUAGE CPP SKIP_INSTALL)
nros_generate_interfaces(unique_identifier_msgs LANGUAGE CPP SKIP_INSTALL)
nros_generate_interfaces(action_msgs LANGUAGE CPP
    DEPENDENCIES builtin_interfaces unique_identifier_msgs SKIP_INSTALL)
nros_generate_interfaces(example_interfaces LANGUAGE CPP
    DEPENDENCIES builtin_interfaces unique_identifier_msgs action_msgs SKIP_INSTALL)

add_executable(cpp_action_client src/main.cpp)
target_link_libraries(cpp_action_client PRIVATE
    example_interfaces__nano_ros_cpp
    NanoRos::NanoRosCpp)
nros_platform_link_app(cpp_action_client)
