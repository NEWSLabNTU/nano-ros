cmake_minimum_required(VERSION 3.22)
project(zenoh_to_dds_cpp_bridge VERSION 0.1.0 LANGUAGES CXX)

set(CMAKE_CXX_STANDARD 14)
set(CMAKE_CXX_STANDARD_REQUIRED ON)

# Phase 104.D.2 — C++ bridge example. Two RMW backends (zenoh +
# dds) linked into one binary. Each registers under its
# canonical name via `.init_array` ctor (Phase 104.A); the C++
# surface picks per node via `executor.node_builder(...).rmw(...)`
# (Phase 104.C.9).
#
# Six levels up from examples/native/cpp/bridge/<example>/ to
# the repo root.
set(NANO_ROS_PLATFORM posix)
# Primary RMW for the root add_subdirectory dispatch — picks the
# Zenoh staticlib branch (auto-pulled via NANO_ROS_RMW=zenoh).
set(NANO_ROS_RMW zenoh)
add_subdirectory("${CMAKE_CURRENT_SOURCE_DIR}/../../../../.." nano_ros)

# Phase 104.D.2 — pull the SECOND backend (dust-DDS) staticlib
# directly via corrosion. Root CMake's NANO_ROS_RMW dispatch
# only handles single-backend builds today (Phase 137.3 inline
# branch); multi-backend bridge examples extend it per-target.
# Mirrors the root `NANO_ROS_RMW STREQUAL "dds"` branch in
# CMakeLists.txt:157-182.
corrosion_import_crate(
    MANIFEST_PATH "${CMAKE_CURRENT_SOURCE_DIR}/../../../../../Cargo.toml"
    CRATES        nros-rmw-dds-staticlib
    CRATE_TYPES   staticlib
    NO_DEFAULT_FEATURES
    FEATURES      platform-posix ros-humble
    LOCKED
)

add_executable(zenoh_to_dds_cpp_bridge src/main.cpp)
target_link_libraries(zenoh_to_dds_cpp_bridge PRIVATE NanoRos::NanoRosCpp)

# Whole-archive wrap on BOTH RMW staticlibs so their
# `.init_array` ctors + linkme entries survive dead-strip.
# Per-target wrap is the bridge workaround for root CMake's
# single-backend `NANO_ROS_RMW` dispatch — see the D.1 C
# bridge for the same pattern. `--allow-multiple-definition`
# handles any duplicate platform-cffi symbols across the two
# backends.
if(CMAKE_SYSTEM_NAME STREQUAL "Linux" OR CMAKE_SYSTEM_NAME MATCHES "BSD")
    target_link_libraries(zenoh_to_dds_cpp_bridge PRIVATE
        "-Wl,--whole-archive"
        nros_rmw_zenoh_staticlib-static
        nros_rmw_dds_staticlib-static
        "-Wl,--no-whole-archive"
        "-Wl,--allow-multiple-definition")
elseif(APPLE)
    target_link_libraries(zenoh_to_dds_cpp_bridge PRIVATE
        "-Wl,-force_load,$<TARGET_FILE:nros_rmw_zenoh_staticlib-static>"
        "-Wl,-force_load,$<TARGET_FILE:nros_rmw_dds_staticlib-static>"
        "-Wl,--allow-multiple-definition")
else()
    target_link_libraries(zenoh_to_dds_cpp_bridge PRIVATE
        nros_rmw_zenoh_staticlib-static
        nros_rmw_dds_staticlib-static)
endif()

nros_platform_link_app(zenoh_to_dds_cpp_bridge)
