cmake_minimum_required(VERSION 3.22)
# Phase 170.A — Cyclone DDS build path for the collapsed pure-Rust
# listener (see native/rust/talker/CMakeLists.txt for the rationale:
# corrosion can't link the C++ backend into a corrosion-imported bin,
# so import the crate's staticlib + link via a C main). idlc emits the
# descriptors as C source, so the project enables C.
# CXX: the Cyclone DDS backend is C++ and nano_ros_link_rmw links this app with
# the C++ driver (LINKER_LANGUAGE CXX), so CXX must be enabled in this scope.
project(native_rust_listener_cyclonedds VERSION 0.1.0 LANGUAGES C CXX)

set(NANO_ROS_PLATFORM posix)
set(NANO_ROS_RMW     cyclonedds)
set(NROS_RMW_CYCLONEDDS_MSG_TO_IDL
    "${CMAKE_CURRENT_SOURCE_DIR}/../../../../scripts/cyclonedds/msg_to_cyclone_idl.py"
    CACHE FILEPATH "nano-ros Cyclone DDS .msg/.srv to IDL converter" FORCE)
add_subdirectory("${CMAKE_CURRENT_SOURCE_DIR}/../../../.." nano_ros)

nros_generate_interfaces(builtin_interfaces SKIP_INSTALL)
nros_generate_interfaces(std_msgs DEPENDENCIES builtin_interfaces SKIP_INSTALL)

corrosion_import_crate(
    MANIFEST_PATH "${CMAKE_CURRENT_SOURCE_DIR}/Cargo.toml"
    CRATES native-rs-listener
    CRATE_TYPES staticlib
    NO_DEFAULT_FEATURES
    FEATURES rmw-cyclonedds)

add_executable(listener_cyclonedds src/cyclonedds_main.c)
target_link_libraries(listener_cyclonedds PRIVATE
    native_rs_listener
    std_msgs__nano_ros_c
    NanoRos::NanoRos
    "-Wl,--allow-multiple-definition"
    "-Wl,--no-as-needed,-lstdc++")
nros_platform_link_app(listener_cyclonedds)
