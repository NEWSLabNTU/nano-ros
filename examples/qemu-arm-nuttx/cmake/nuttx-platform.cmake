# nuttx-platform.cmake
#
# Shared CMake module for NuttX C/C++ examples (QEMU ARM virt).
#
# Unlike FreeRTOS, NuttX doesn't need RTOS kernel compilation — the kernel
# is built into the Rust binary via `-Z build-std=std`. This module:
#   1. Generates message bindings via nros_generate_interfaces()
#   2. Builds the final NuttX kernel ELF via `cargo +nightly build` on the
#      nros-nuttx-ffi crate, which compiles app_main() from C++ via build.rs
#
# Provides:
#   nuttx_build_example(<name> <main_cpp> [INCLUDE_DIRS <dirs>...])
#     — builds a NuttX kernel ELF with the given C++ source

get_filename_component(_NUTTX_CMAKE_DIR "${CMAKE_CURRENT_LIST_FILE}" DIRECTORY)
get_filename_component(_NROS_ROOT "${_NUTTX_CMAKE_DIR}/../../.." ABSOLUTE)

set(_FFI_CRATE_DIR "${_NUTTX_CMAKE_DIR}/nros-nuttx-ffi")

# ============================================================================
# Codegen — nros_generate_interfaces()
# ============================================================================

set(_CODEGEN_CRATE "${_NROS_ROOT}/packages/codegen/packages/nros-codegen-c")
set(_CODEGEN_TARGET_DIR "${_NROS_ROOT}/packages/codegen/packages/target")
find_program(_NANO_ROS_CODEGEN_TOOL nros-codegen
    PATHS "${_CODEGEN_TARGET_DIR}/release" "${_CODEGEN_TARGET_DIR}/debug"
    NO_DEFAULT_PATH
)
if(NOT _NANO_ROS_CODEGEN_TOOL)
    message(STATUS "nros-codegen not found, building...")
    execute_process(
        COMMAND cargo build --manifest-path "${_CODEGEN_CRATE}/Cargo.toml" --release
        WORKING_DIRECTORY "${_NROS_ROOT}"
        RESULT_VARIABLE _codegen_result
    )
    if(NOT _codegen_result EQUAL 0)
        message(FATAL_ERROR "Failed to build nros-codegen")
    endif()
    set(_NANO_ROS_CODEGEN_TOOL "${_CODEGEN_TARGET_DIR}/release/nros-codegen")
endif()
set(_NANO_ROS_CODEGEN_TOOL "${_NANO_ROS_CODEGEN_TOOL}" CACHE INTERNAL "")
message(STATUS "Found nros codegen tool: ${_NANO_ROS_CODEGEN_TOOL}")

# Set Rust_CARGO_TARGET for per-message FFI cross-compilation
set(Rust_CARGO_TARGET "armv7a-nuttx-eabi")

# Paths for codegen
set(_NANO_ROS_PREFIX "${_NROS_ROOT}")
set(_NANO_ROS_CMAKE_DIR "${_NROS_ROOT}/packages/codegen/packages/nros-codegen-c/cmake")

if(NOT EXISTS "${_NROS_ROOT}/share/nano-ros/rust/nros-serdes/src")
    set(_serdes_src "${_NROS_ROOT}/packages/core/nros-serdes")
    file(MAKE_DIRECTORY "${_NROS_ROOT}/share/nano-ros/rust")
    if(NOT EXISTS "${_NROS_ROOT}/share/nano-ros/rust/nros-serdes")
        file(CREATE_LINK "${_serdes_src}" "${_NROS_ROOT}/share/nano-ros/rust/nros-serdes" SYMBOLIC)
    endif()
endif()
if(NOT EXISTS "${_NROS_ROOT}/share/nano-ros/interfaces")
    file(CREATE_LINK "${_NROS_ROOT}/packages/codegen/interfaces"
         "${_NROS_ROOT}/share/nano-ros/interfaces" SYMBOLIC)
endif()

include("${_NANO_ROS_CMAKE_DIR}/NanoRosGenerateInterfaces.cmake")
include("${_NROS_ROOT}/cmake/NanoRosConfig.cmake")

# ============================================================================
# nuttx_build_example() — builds a NuttX kernel ELF with C++ app
# ============================================================================
#
# Usage:
#   nuttx_build_example(<name> <main_cpp>
#       [INCLUDE_DIRS <dir1> <dir2> ...]
#   )
#
# This function:
#   1. Runs `cargo +nightly build --release` on nros-nuttx-ffi
#   2. Sets APP_MAIN_CPP to the C++ source file
#   3. Sets APP_INCLUDE_DIRS to the codegen include directories
#   4. The resulting binary is at nros-nuttx-ffi/target/armv7a-nuttx-eabi/release/nros-nuttx-ffi

function(nuttx_build_example name main_cpp)
    cmake_parse_arguments(_ARG "" "" "INCLUDE_DIRS;COMPILE_DEFS" ${ARGN})

    # Collect include dirs (codegen output + user-specified)
    set(_include_dirs "${_NROS_ROOT}/packages/core/nros-cpp/include")
    foreach(_dir ${_ARG_INCLUDE_DIRS})
        list(APPEND _include_dirs "${_dir}")
    endforeach()
    string(JOIN ";" _include_dirs_str ${_include_dirs})

    # Collect compile definitions (from config.toml via nano_ros_read_config)
    set(_compile_defs "")
    foreach(_def ${_ARG_COMPILE_DEFS})
        list(APPEND _compile_defs "${_def}")
    endforeach()
    string(JOIN ";" _compile_defs_str ${_compile_defs})

    set(_output_binary "${_FFI_CRATE_DIR}/target/armv7a-nuttx-eabi/release/nros-nuttx-ffi")

    add_custom_command(
        OUTPUT "${_output_binary}"
        COMMAND ${CMAKE_COMMAND} -E env
            "APP_MAIN_CPP=${main_cpp}"
            "APP_INCLUDE_DIRS=${_include_dirs_str}"
            "APP_COMPILE_DEFS=${_compile_defs_str}"
            "NUTTX_DIR=${NUTTX_DIR}"
            "NUTTX_APPS_DIR=${NUTTX_DIR}/../nuttx-apps"
            cargo +nightly build --release
        WORKING_DIRECTORY "${_FFI_CRATE_DIR}"
        DEPENDS "${main_cpp}"
        COMMENT "Building NuttX C++ example: ${name}"
        VERBATIM
    )

    add_custom_target(${name}_build ALL DEPENDS "${_output_binary}")

    # Ensure codegen runs before the cargo build (caller adds dependencies via
    # add_dependencies(${name}_build, <codegen_target>) after this call)

    # Copy the binary to the build directory for convenience
    add_custom_command(
        TARGET ${name}_build POST_BUILD
        COMMAND ${CMAKE_COMMAND} -E copy "${_output_binary}" "${CMAKE_CURRENT_BINARY_DIR}/${name}"
        COMMENT "Copying ${name} to build directory"
    )
endfunction()
