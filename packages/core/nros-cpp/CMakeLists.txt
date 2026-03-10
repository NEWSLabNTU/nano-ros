# packages/core/nros-cpp/CMakeLists.txt
#
# Header-only C++ library for nros.
# Provides the NanoRos::NanoRosCpp target.
#
# This file is included from the top-level CMakeLists.txt via add_subdirectory().
# It depends on the nros-cpp-ffi static library (built via Corrosion).

# ---- Map RMW name to Cargo features (same as nros-c) ----
if(NANO_ROS_RMW STREQUAL "zenoh")
    set(_cpp_ffi_features std rmw-zenoh platform-posix ros-humble)
elseif(NANO_ROS_RMW STREQUAL "xrce")
    set(_cpp_ffi_features std rmw-xrce xrce-udp platform-posix ros-humble)
else()
    message(FATAL_ERROR "Unknown NANO_ROS_RMW: ${NANO_ROS_RMW} (expected: zenoh or xrce)")
endif()

# ---- Build nros-cpp-ffi via Corrosion ----
corrosion_import_crate(
    MANIFEST_PATH "${PROJECT_SOURCE_DIR}/Cargo.toml"
    CRATES        nros-cpp-ffi
    CRATE_TYPES   staticlib
    NO_DEFAULT_FEATURES
    FEATURES      ${_cpp_ffi_features}
    LOCKED
)

# ---- Header-only C++ interface library ----
add_library(nros-cpp INTERFACE)
add_library(NanoRos::NanoRosCpp ALIAS nros-cpp)

target_include_directories(nros-cpp INTERFACE
    $<BUILD_INTERFACE:${CMAKE_CURRENT_SOURCE_DIR}/include>
    $<INSTALL_INTERFACE:include>
)

# Link against the FFI static library
target_link_libraries(nros-cpp INTERFACE nros_cpp_ffi-static)

# Require at least C++14
target_compile_features(nros-cpp INTERFACE cxx_std_14)

# ---- Install ----
install(DIRECTORY "${CMAKE_CURRENT_SOURCE_DIR}/include/nros"
    DESTINATION include
    FILES_MATCHING PATTERN "*.hpp"
)

install(FILES "$<TARGET_FILE:nros_cpp_ffi-static>"
    DESTINATION lib
    RENAME "libnros_cpp_ffi_${NANO_ROS_RMW}.a"
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
