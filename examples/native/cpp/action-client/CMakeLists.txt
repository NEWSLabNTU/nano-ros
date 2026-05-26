cmake_minimum_required(VERSION 3.22)
project(cpp_action_client LANGUAGES C CXX)

set(CMAKE_CXX_STANDARD 14)
set(CMAKE_CXX_STANDARD_REQUIRED ON)

# Phase 118.B.3 — collapsed-shape C++ action-client. RMW selected at
# cmake configure time via `-DNROS_RMW=<rmw>` (default zenoh).
set(NANO_ROS_PLATFORM posix)
set(NROS_RMW "zenoh" CACHE STRING
    "Active RMW (zenoh|dds|xrce|cyclonedds) — selects the backend linked into this example.")
set(NANO_ROS_RMW "${NROS_RMW}")
add_subdirectory("${CMAKE_CURRENT_SOURCE_DIR}/../../../.." nano_ros)

nros_generate_interfaces(builtin_interfaces LANGUAGE CPP SKIP_INSTALL)
nros_generate_interfaces(std_msgs DEPENDENCIES builtin_interfaces LANGUAGE CPP SKIP_INSTALL)
# Cyclone needs IDL descriptors for the shared action_msgs types (CancelGoal +
# GoalStatusArray/GoalStatus/GoalInfo) that every action's cancel service and
# status topic use; action_msgs/GoalInfo needs unique_identifier_msgs (UUID).
# Generate them and chain example_interfaces' dependency so the descriptor
# ts-libs whole-archive in. zenoh/xrce don't use IDL descriptors — harmless.
nros_generate_interfaces(unique_identifier_msgs LANGUAGE CPP SKIP_INSTALL)
nros_generate_interfaces(action_msgs DEPENDENCIES builtin_interfaces unique_identifier_msgs LANGUAGE CPP SKIP_INSTALL)
nros_generate_interfaces(example_interfaces DEPENDENCIES builtin_interfaces action_msgs LANGUAGE CPP SKIP_INSTALL)



add_executable(cpp_action_client src/main.cpp)
target_link_libraries(cpp_action_client PRIVATE
    example_interfaces__nano_ros_cpp
    NanoRos::NanoRosCpp)
nros_platform_link_app(cpp_action_client)
