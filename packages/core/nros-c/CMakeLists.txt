# packages/core/nros-c/CMakeLists.txt
#
# Builds the nros-c static library via Corrosion (Cargo integration).
# Installs library (RMW-suffixed), headers, and CMake config package.
#
# This file is included from the top-level CMakeLists.txt via add_subdirectory().
# The NANO_ROS_RMW variable (zenoh or xrce) selects features.

# ---- Map RMW name to Cargo features ----
if(NANO_ROS_RMW STREQUAL "zenoh")
    set(_features std rmw-zenoh platform-posix ros-humble)
elseif(NANO_ROS_RMW STREQUAL "xrce")
    set(_features std rmw-xrce xrce-udp platform-posix ros-humble)
else()
    message(FATAL_ERROR "Unknown NANO_ROS_RMW: ${NANO_ROS_RMW} (expected: zenoh or xrce)")
endif()

# ---- Build nros-c via Corrosion ----
corrosion_import_crate(
    MANIFEST_PATH "${PROJECT_SOURCE_DIR}/Cargo.toml"
    CRATES        nros-c
    CRATE_TYPES   staticlib
    NO_DEFAULT_FEATURES
    FEATURES      ${_features}
    LOCKED
)

# ---- Install library with RMW-suffixed name ----
# Corrosion creates target "nros_c-static" (the IMPORTED STATIC library)
# from crate "nros-c". We install as libnros_c_<rmw>.a so multiple
# RMW variants coexist in the same prefix.
install(FILES "$<TARGET_FILE:nros_c-static>"
    DESTINATION lib
    RENAME "libnros_c_${NANO_ROS_RMW}.a"
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
    DESTINATION lib/cmake/NanoRos
)
