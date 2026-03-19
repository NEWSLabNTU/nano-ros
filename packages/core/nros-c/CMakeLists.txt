# packages/core/nros-c/CMakeLists.txt
#
# Builds the nros-c static library via Corrosion (Cargo integration).
# Installs library (RMW + platform suffixed), headers, and CMake config package.
#
# This file is included from the top-level CMakeLists.txt via add_subdirectory().
# Two variables control the build:
#   NANO_ROS_RMW      — RMW backend: zenoh, xrce, or dds (default: zenoh)
#   NANO_ROS_PLATFORM — target platform: posix, freertos_armcm3, nuttx_armv7a
#                       (default: posix)

# ---- Platform (default: posix) ----
if(NOT DEFINED NANO_ROS_PLATFORM)
    set(NANO_ROS_PLATFORM "posix")
endif()

# ---- Map RMW to Cargo features ----
if(NANO_ROS_RMW STREQUAL "zenoh")
    set(_rmw_features rmw-zenoh)
elseif(NANO_ROS_RMW STREQUAL "xrce")
    set(_rmw_features rmw-xrce xrce-udp)
elseif(NANO_ROS_RMW STREQUAL "dds")
    set(_rmw_features rmw-dds)
else()
    message(FATAL_ERROR "Unknown NANO_ROS_RMW: ${NANO_ROS_RMW} (expected: zenoh, xrce, or dds)")
endif()

# ---- Map platform to Cargo features ----
if(NANO_ROS_PLATFORM STREQUAL "posix")
    set(_platform_features std platform-posix)
elseif(NANO_ROS_PLATFORM STREQUAL "freertos_armcm3")
    # FreeRTOS bare-metal: no std, alloc for internal Rust collections,
    # panic-halt for a minimal panic handler, link-tcp/link-udp for zenoh-pico.
    set(_platform_features alloc panic-halt platform-freertos link-tcp link-udp-unicast)
elseif(NANO_ROS_PLATFORM STREQUAL "nuttx_armv7a")
    # NuttX provides a POSIX-like libc, so std is available.
    set(_platform_features std platform-nuttx)
else()
    message(FATAL_ERROR "Unknown NANO_ROS_PLATFORM: ${NANO_ROS_PLATFORM} "
            "(expected: posix, freertos_armcm3, or nuttx_armv7a)")
endif()

set(_features ros-humble ${_rmw_features} ${_platform_features})

# ---- Build nros-c via Corrosion ----
corrosion_import_crate(
    MANIFEST_PATH "${PROJECT_SOURCE_DIR}/Cargo.toml"
    CRATES        nros-c
    CRATE_TYPES   staticlib
    NO_DEFAULT_FEATURES
    FEATURES      ${_features}
    LOCKED
)

# ---- Install library ----
# posix uses the legacy unsuffixed name (libnros_c_<rmw>.a) for backwards
# compatibility.  All other platforms use a suffixed name so multiple
# variants coexist in the same prefix.
if(NANO_ROS_PLATFORM STREQUAL "posix")
    set(_lib_name "libnros_c_${NANO_ROS_RMW}.a")
else()
    set(_lib_name "libnros_c_${NANO_ROS_RMW}_${NANO_ROS_PLATFORM}.a")
endif()

install(FILES "$<TARGET_FILE:nros_c-static>"
    DESTINATION lib
    RENAME "${_lib_name}"
)

# ---- Headers ----
install(DIRECTORY "${CMAKE_CURRENT_SOURCE_DIR}/include/nros"
    DESTINATION include
    FILES_MATCHING PATTERN "*.h"
)

# ---- CMake config-mode package ----
install(FILES
    "${CMAKE_CURRENT_SOURCE_DIR}/cmake/NanoRosConfig.cmake"
    "${CMAKE_CURRENT_SOURCE_DIR}/cmake/NanoRosCTargets.cmake"
    "${CMAKE_CURRENT_SOURCE_DIR}/cmake/NanoRosReadConfig.cmake"
    DESTINATION lib/cmake/NanoRos
)
