cmake_minimum_required(VERSION 3.22)
# Phase 170.A — Cyclone DDS build path for the collapsed pure-Rust
# service-client (see native/rust/talker/CMakeLists.txt for rationale).
project(native_rust_service_client_cyclonedds VERSION 0.1.0 LANGUAGES C)

set(NANO_ROS_PLATFORM posix)
set(NANO_ROS_RMW     cyclonedds)
set(NROS_RMW_CYCLONEDDS_MSG_TO_IDL
    "${CMAKE_CURRENT_SOURCE_DIR}/../../../../scripts/cyclonedds/msg_to_cyclone_idl.py"
    CACHE FILEPATH "nano-ros Cyclone DDS .msg/.srv to IDL converter" FORCE)
add_subdirectory("${CMAKE_CURRENT_SOURCE_DIR}/../../../.." nano_ros)

nros_generate_interfaces(example_interfaces SKIP_INSTALL)

corrosion_import_crate(
    MANIFEST_PATH "${CMAKE_CURRENT_SOURCE_DIR}/Cargo.toml"
    CRATES native-rs-service-client
    CRATE_TYPES staticlib
    NO_DEFAULT_FEATURES
    FEATURES rmw-cyclonedds)

add_executable(service_client_cyclonedds src/cyclonedds_main.c)
target_link_libraries(service_client_cyclonedds PRIVATE
    native_rs_service_client
    example_interfaces__nano_ros_c
    NanoRos::NanoRos
    "-Wl,--allow-multiple-definition"
    "-Wl,--no-as-needed,-lstdc++")
nros_platform_link_app(service_client_cyclonedds)
