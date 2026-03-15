# packages/core/nros-cpp/CMakeLists.txt
#
# nros C++ API: header-only C++14 library + Rust FFI staticlib.
# Provides the NanoRos::NanoRosCpp target.
#
# This file is included from the top-level CMakeLists.txt via add_subdirectory().

# ---- Map RMW name to Cargo features (same as nros-c) ----
if(NANO_ROS_RMW STREQUAL "zenoh")
    set(_cpp_features std rmw-zenoh platform-posix ros-humble)
elseif(NANO_ROS_RMW STREQUAL "xrce")
    set(_cpp_features std rmw-xrce xrce-udp platform-posix ros-humble)
else()
    message(FATAL_ERROR "Unknown NANO_ROS_RMW: ${NANO_ROS_RMW} (expected: zenoh or xrce)")
endif()

# ---- Build nros-cpp via Corrosion ----
corrosion_import_crate(
    MANIFEST_PATH "${PROJECT_SOURCE_DIR}/Cargo.toml"
    CRATES        nros-cpp
    CRATE_TYPES   staticlib
    NO_DEFAULT_FEATURES
    FEATURES      ${_cpp_features}
    LOCKED
)

# ---- Header-only C++ interface library ----
add_library(nros-cpp-headers INTERFACE)
add_library(NanoRos::NanoRosCpp ALIAS nros-cpp-headers)

target_include_directories(nros-cpp-headers INTERFACE
    $<BUILD_INTERFACE:${CMAKE_CURRENT_SOURCE_DIR}/include>
    $<INSTALL_INTERFACE:include>
)

# Link against the FFI static library
target_link_libraries(nros-cpp-headers INTERFACE nros_cpp-static)

# Require at least C++14
target_compile_features(nros-cpp-headers INTERFACE cxx_std_14)

# ---- Install ----
install(DIRECTORY "${CMAKE_CURRENT_SOURCE_DIR}/include/nros"
    DESTINATION include
    FILES_MATCHING PATTERN "*.hpp" PATTERN "*.h"
)

install(FILES "$<TARGET_FILE:nros_cpp-static>"
    DESTINATION lib
    RENAME "libnros_cpp_${NANO_ROS_RMW}.a"
)

install(FILES "${CMAKE_CURRENT_SOURCE_DIR}/cmake/NanoRosCppTargets.cmake"
    DESTINATION lib/cmake/NanoRos
)

# Install nros-serdes source for standalone C++ codegen builds.
# The NanoRosGenerateInterfaces.cmake creates a temporary Cargo project
# that depends on nros-serdes to compile the generated .rs FFI glue.
# We install the Rust source files and a standalone Cargo.toml (without
# workspace references).
install(DIRECTORY "${PROJECT_SOURCE_DIR}/packages/core/nros-serdes/src"
    DESTINATION share/nano-ros/rust/nros-serdes
)
install(FILES "${CMAKE_CURRENT_SOURCE_DIR}/cmake/nros-serdes-standalone-Cargo.toml"
    DESTINATION share/nano-ros/rust/nros-serdes
    RENAME Cargo.toml
)
