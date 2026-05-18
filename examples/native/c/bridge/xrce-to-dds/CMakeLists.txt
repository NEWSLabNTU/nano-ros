cmake_minimum_required(VERSION 3.22)
project(xrce_to_dds_bridge LANGUAGES C)

set(CMAKE_C_STANDARD 11)
set(CMAKE_C_STANDARD_REQUIRED ON)

# Phase 104.D.1 — C bridge example. Two RMW backends (XRCE + DDS)
# linked into one binary. Each registers under its canonical name
# via `.init_array` ctor (Phase 104.A); the C surface picks per
# node via `nros_node_options_t.rmw_name` (Phase 104.C.8).
#
# Six levels up from examples/native/c/bridge/<example>/ to the
# repo root.
set(NANO_ROS_PLATFORM posix)
# Primary RMW for the root add_subdirectory dispatch — picks the
# XRCE staticlib branch (auto-pulled via NANO_ROS_RMW=xrce).
set(NANO_ROS_RMW xrce)
add_subdirectory("${CMAKE_CURRENT_SOURCE_DIR}/../../../../.." nano_ros)

# Phase 104.D.1 — pull the SECOND backend (dust-DDS) staticlib
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

add_executable(xrce_to_dds_bridge src/main.c)
target_link_libraries(xrce_to_dds_bridge PRIVATE NanoRos::NanoRos)

# Whole-archive wrap on BOTH RMW staticlibs so their
# `.init_array` ctors (`nros_rmw_xrce_register` /
# `nros_rmw_dds_register`) + linkme entries survive dead-strip.
# Root CMake's NANO_ROS_RMW=xrce branch wraps the xrce staticlib
# inside `NanoRos::NanoRos`'s INTERFACE_LINK_LIBRARIES, but
# CMake's per-target link-line dedup can split the wrap tokens
# from the staticlib once the INTERFACE chain expands here —
# add an explicit redundant wrap so the xrce_cffi members the
# linkme registry needs always land in the final binary.
# `--allow-multiple-definition` covers any duplicate
# platform-cffi symbols across the two backends.
if(CMAKE_SYSTEM_NAME STREQUAL "Linux" OR CMAKE_SYSTEM_NAME MATCHES "BSD")
    target_link_libraries(xrce_to_dds_bridge PRIVATE
        "-Wl,--whole-archive"
        nros_rmw_xrce_cffi-static
        nros_rmw_dds_staticlib-static
        "-Wl,--no-whole-archive"
        "-Wl,--allow-multiple-definition")
elseif(APPLE)
    target_link_libraries(xrce_to_dds_bridge PRIVATE
        "-Wl,-force_load,$<TARGET_FILE:nros_rmw_xrce_cffi-static>"
        "-Wl,-force_load,$<TARGET_FILE:nros_rmw_dds_staticlib-static>"
        "-Wl,--allow-multiple-definition")
else()
    target_link_libraries(xrce_to_dds_bridge PRIVATE
        nros_rmw_xrce_cffi-static
        nros_rmw_dds_staticlib-static)
endif()

nros_platform_link_app(xrce_to_dds_bridge)
