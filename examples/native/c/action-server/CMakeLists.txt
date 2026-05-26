cmake_minimum_required(VERSION 3.22)
project(c_action_server LANGUAGES C)

set(CMAKE_C_STANDARD 11)
set(CMAKE_C_STANDARD_REQUIRED ON)

# Phase 118.B.2 — collapsed-shape C action-server. RMW selected at cmake
# configure time via `-DNROS_RMW=<rmw>` (default zenoh).
set(NANO_ROS_PLATFORM posix)
set(NROS_RMW "zenoh" CACHE STRING
    "Active RMW (zenoh|dds|xrce|cyclonedds) — selects the backend linked into this example.")
set(NANO_ROS_RMW "${NROS_RMW}")

# The Cyclone DDS backend is C++ (operator new/delete, std::nothrow). A C app
# linking it gets LINKER_LANGUAGE CXX (set by nano_ros_link_rmw), which needs
# CMAKE_CXX_LINK_EXECUTABLE — i.e. CXX enabled in *this* directory scope. The
# nano-ros add_subdirectory enables CXX only in its own subtree, so enable it
# here for the Cyclone RMW. (zenoh / xrce stay pure C.)
if(NROS_RMW STREQUAL "cyclonedds")
    enable_language(CXX)
endif()

add_subdirectory("${CMAKE_CURRENT_SOURCE_DIR}/../../../.." nano_ros)

nros_generate_interfaces(builtin_interfaces SKIP_INSTALL)
nros_generate_interfaces(std_msgs DEPENDENCIES builtin_interfaces SKIP_INSTALL)
# Cyclone needs IDL descriptors for the shared action_msgs types (CancelGoal +
# GoalStatusArray/GoalStatus/GoalInfo) that every action's cancel service and
# status topic use. Generate action_msgs and declare it as an example_interfaces
# dependency so its descriptor ts-lib is whole-archived into the app (the cancel
# service / status pub otherwise fail to create on Cyclone). zenoh/xrce don't
# use IDL descriptors — harmless there.
nros_generate_interfaces(unique_identifier_msgs SKIP_INSTALL)
nros_generate_interfaces(action_msgs DEPENDENCIES builtin_interfaces unique_identifier_msgs SKIP_INSTALL)
nros_generate_interfaces(example_interfaces DEPENDENCIES builtin_interfaces action_msgs SKIP_INSTALL)



add_executable(c_action_server src/main.c)
target_link_libraries(c_action_server PRIVATE
    example_interfaces__nano_ros_c
    NanoRos::NanoRos)
nros_platform_link_app(c_action_server)
