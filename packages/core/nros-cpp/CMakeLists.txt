# packages/core/nros-cpp/CMakeLists.txt
#
# nros C++ API: header-only C++14 library + Rust FFI staticlib.
# Provides the NanoRos::NanoRosCpp target.
#
# This file is included from the top-level CMakeLists.txt via add_subdirectory().
# Two variables control the build:
#   NANO_ROS_RMW      — RMW backend: zenoh, xrce, or dds (default: zenoh)
#   NANO_ROS_PLATFORM — target platform: posix, freertos_armcm3, nuttx_armv7a
#                       (default: posix)

# ---- Platform (default: posix; may already be set by nros-c) ----
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
    set(_platform_features alloc panic-halt platform-freertos link-tcp link-udp-unicast)
elseif(NANO_ROS_PLATFORM STREQUAL "nuttx_armv7a")
    set(_platform_features std platform-nuttx link-tcp link-udp-unicast)
elseif(NANO_ROS_PLATFORM STREQUAL "threadx_linux")
    set(_platform_features std platform-threadx link-tcp link-udp-unicast)
elseif(NANO_ROS_PLATFORM STREQUAL "threadx_riscv64")
    set(_platform_features alloc panic-halt platform-threadx link-tcp link-udp-unicast)
else()
    message(FATAL_ERROR "Unknown NANO_ROS_PLATFORM: ${NANO_ROS_PLATFORM} "
            "(expected: posix, freertos_armcm3, nuttx_armv7a, or threadx_linux)")
endif()

set(_cpp_features ros-humble ${_rmw_features} ${_platform_features})

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

# posix uses the legacy unsuffixed name; all other platforms use a suffixed name.
if(NANO_ROS_PLATFORM STREQUAL "posix")
    set(_cpp_lib_name "libnros_cpp_${NANO_ROS_RMW}.a")
else()
    set(_cpp_lib_name "libnros_cpp_${NANO_ROS_RMW}_${NANO_ROS_PLATFORM}.a")
endif()

install(FILES "$<TARGET_FILE:nros_cpp-static>"
    DESTINATION lib
    RENAME "${_cpp_lib_name}"
)

# Strip soft-float compiler_builtins from RISC-V archive after installing.
if(NANO_ROS_PLATFORM STREQUAL "threadx_riscv64")
    execute_process(COMMAND rustc --print sysroot
        OUTPUT_VARIABLE _RUST_SYSROOT_CPP OUTPUT_STRIP_TRAILING_WHITESPACE ERROR_QUIET)
    find_program(_LLVM_AR_CPP llvm-ar
        PATHS "${_RUST_SYSROOT_CPP}/lib/rustlib/x86_64-unknown-linux-gnu/bin" NO_DEFAULT_PATH)
    if(_LLVM_AR_CPP)
        set(_strip_script_cpp "${PROJECT_SOURCE_DIR}/cmake/strip-compiler-builtins.sh")
        install(CODE "execute_process(COMMAND bash \"${_strip_script_cpp}\" \"${_LLVM_AR_CPP}\" \"\${CMAKE_INSTALL_PREFIX}/lib/${_cpp_lib_name}\")")
    endif()
endif()

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
