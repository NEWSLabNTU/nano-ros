cmake_minimum_required(VERSION 3.22)
project(NrosRmwZenoh VERSION 0.1.0 LANGUAGES C)

# Phase 140 — install rules + Config.cmake.in template deleted.
# Consumed in-tree via `add_subdirectory(...)` from the root
# `CMakeLists.txt` zenoh branch, which links `nros_rmw_zenoh_staticlib-static`
# (the Corrosion target below) into `NanoRos::NanoRos` directly.

if(NANO_ROS_PLATFORM STREQUAL "posix")
    set(_platform_features std platform-posix)
    set(_lib_name "libnros_rmw_zenoh.a")
elseif(NANO_ROS_PLATFORM STREQUAL "freertos_armcm3"
       OR NANO_ROS_PLATFORM STREQUAL "freertos")
    # Phase 138 canonical "freertos" accepted alongside legacy
    # "freertos_armcm3"; Phase 138 collapsed board into NANO_ROS_BOARD.
    set(_platform_features platform-freertos)
    set(_lib_name "libnros_rmw_zenoh_${NANO_ROS_PLATFORM}.a")
elseif(NANO_ROS_PLATFORM STREQUAL "nuttx_armv7a"
       OR NANO_ROS_PLATFORM STREQUAL "nuttx")
    # Phase 138 canonical "nuttx" accepted alongside legacy
    # "nuttx_armv7a"; Phase 138 collapsed board into NANO_ROS_BOARD.
    set(_platform_features platform-nuttx)
    set(_lib_name "libnros_rmw_zenoh_${NANO_ROS_PLATFORM}.a")
elseif(NANO_ROS_PLATFORM STREQUAL "threadx_linux")
    set(_platform_features platform-threadx-std)
    set(_lib_name "libnros_rmw_zenoh_${NANO_ROS_PLATFORM}.a")
elseif(NANO_ROS_PLATFORM STREQUAL "threadx_riscv64")
    set(_platform_features platform-threadx)
    set(_lib_name "libnros_rmw_zenoh_${NANO_ROS_PLATFORM}.a")
elseif(NANO_ROS_PLATFORM STREQUAL "threadx")
    # Phase 144.7-8 canonical "threadx" — board axis disambiguates
    # the std-vs-no_std crate-feature split.
    if(NANO_ROS_BOARD STREQUAL "threadx-linux")
        set(_platform_features platform-threadx-std)
    elseif(NANO_ROS_BOARD STREQUAL "riscv64-qemu")
        set(_platform_features platform-threadx)
    else()
        message(FATAL_ERROR
            "NANO_ROS_PLATFORM=threadx requires NANO_ROS_BOARD to be one of: "
            "threadx-linux, riscv64-qemu. Got '${NANO_ROS_BOARD}'.")
    endif()
    set(_lib_name "libnros_rmw_zenoh_${NANO_ROS_PLATFORM}.a")
else()
    message(FATAL_ERROR "Unknown NANO_ROS_PLATFORM: ${NANO_ROS_PLATFORM}")
endif()

corrosion_import_crate(
    MANIFEST_PATH "${PROJECT_SOURCE_DIR}/Cargo.toml"
    CRATES        nros-rmw-zenoh-staticlib
    CRATE_TYPES   staticlib
    NO_DEFAULT_FEATURES
    FEATURES      ros-humble ${_platform_features}
    LOCKED
)

# _lib_name retained above for reference only — the in-tree consumer
# links the Corrosion staticlib target directly under the
# `nros_rmw_zenoh_staticlib-static` name (no rename needed).
set(NROS_RMW_ZENOH_LIB_NAME "${_lib_name}")
