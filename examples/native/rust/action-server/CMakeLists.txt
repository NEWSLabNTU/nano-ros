cmake_minimum_required(VERSION 3.22)
# Phase 170.A — Cyclone DDS build path for the collapsed pure-Rust
# action-server (see native/rust/talker/CMakeLists.txt for rationale).
project(native_rust_action_server_cyclonedds VERSION 0.1.0 LANGUAGES C)

set(NANO_ROS_PLATFORM posix)
set(NANO_ROS_RMW     cyclonedds)
add_subdirectory("${CMAKE_CURRENT_SOURCE_DIR}/../../../.." nano_ros)

# Actions pull the action-infrastructure descriptors (cancel_goal ->
# action_msgs/CancelGoal, status -> GoalStatusArray, transitively
# unique_identifier_msgs/UUID + builtin_interfaces).
nros_generate_interfaces(builtin_interfaces SKIP_INSTALL)
nros_generate_interfaces(unique_identifier_msgs SKIP_INSTALL)
nros_generate_interfaces(action_msgs
    DEPENDENCIES builtin_interfaces unique_identifier_msgs SKIP_INSTALL)
nros_generate_interfaces(example_interfaces
    DEPENDENCIES builtin_interfaces action_msgs SKIP_INSTALL)

corrosion_import_crate(
    MANIFEST_PATH "${CMAKE_CURRENT_SOURCE_DIR}/Cargo.toml"
    CRATES native-rs-action-server
    CRATE_TYPES staticlib
    NO_DEFAULT_FEATURES
    FEATURES rmw-cyclonedds)

add_executable(action_server_cyclonedds src/cyclonedds_main.c)
target_link_libraries(action_server_cyclonedds PRIVATE
    native_rs_action_server
    example_interfaces__nano_ros_c
    NanoRos::NanoRos
    "-Wl,--allow-multiple-definition"
    "-Wl,--no-as-needed,-lstdc++")
nros_platform_link_app(action_server_cyclonedds)
