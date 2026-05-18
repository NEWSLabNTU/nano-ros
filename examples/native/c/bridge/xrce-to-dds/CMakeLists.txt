cmake_minimum_required(VERSION 3.22)
project(xrce_to_dds_bridge LANGUAGES C)

set(CMAKE_C_STANDARD 11)
set(CMAKE_C_STANDARD_REQUIRED ON)

# Phase 104.D.1 — C bridge example. Two RMW backends (XRCE + DDS)
# linked into one binary. The C surface picks per node via
# `nros_node_options_t.rmw_name` (Phase 104.C.8).
#
# Phase 156 Option 3 — `NANO_ROS_RMW=none` tells the root CMake
# to skip its auto-pull of any single RMW staticlib. The bridge
# pulls both backends manually below with IDENTICAL feature sets
# so `nros-rmw-cffi` Cargo-monomorphises to ONE crate instance
# across both staticlib builds (otherwise linkme's
# `#[distributed_slice(RMW_INIT_ENTRIES)]` panics with
# "duplicate distributed_slice" at runtime — two crate hashes
# = two slice definitions). The auto-register-via-linkme path
# is disabled in this mode; main.c calls the explicit
# `nros_rmw_xrce_register()` + `nros_rmw_dds_register()` C
# entry points before `nros_support_init` (Phase 104.A pattern,
# already what the Rust bridge example does — see Phase 128.B.1).
set(NANO_ROS_PLATFORM posix)
set(NANO_ROS_RMW none)
add_subdirectory("${CMAKE_CURRENT_SOURCE_DIR}/../../../../.." nano_ros)

# Pull XRCE-cffi + DDS staticlib explicitly with matching feature
# sets. Both: `NO_DEFAULT_FEATURES` + `platform-posix` (DDS) +
# nothing for XRCE since XRCE-cffi's defaults pull
# `nros-rmw-cffi/linkme-register` which is exactly what we want
# to AVOID — auto-registration off, explicit register on.
corrosion_import_crate(
    MANIFEST_PATH "${CMAKE_CURRENT_SOURCE_DIR}/../../../../../Cargo.toml"
    CRATES        nros-rmw-xrce-cffi
    CRATE_TYPES   staticlib
    NO_DEFAULT_FEATURES
    FEATURES      std
    LOCKED
)
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
# `nros_rmw_<backend>_register` symbols (called explicitly from
# main.c) plus the full dependency closure survive dead-strip.
# `--allow-multiple-definition` covers duplicate platform-cffi
# symbols across the two backends. Note: no linkme entries to
# preserve here since `linkme-register` is disabled — explicit
# `register()` calls do the work.
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
