# nuttx-support.cmake
#
# Shared CMake support module for NuttX C/C++ examples (QEMU ARM virt).
#
# Unlike FreeRTOS, NuttX doesn't need RTOS kernel compilation — the kernel
# is built into the Rust binary via `-Z build-std=std`. This module provides:
#
#   nuttx_build_example(<name> <main_cpp> [INCLUDE_DIRS <dirs>...])
#     — builds a NuttX kernel ELF with the given C++ source
#
# The caller must first call:
#   find_package(NanoRos CONFIG REQUIRED)
# which provides the codegen function (nros_generate_interfaces / nros_find_interfaces)
# and the nros-cpp include directory via NanoRos_DIR.

get_filename_component(_NUTTX_CMAKE_DIR "${CMAKE_CURRENT_LIST_FILE}" DIRECTORY)
set(_FFI_CRATE_DIR "${_NUTTX_CMAKE_DIR}/nros-nuttx-ffi")

# Derive nros-cpp include directory from the install prefix.
# After find_package(NanoRos), NanoRos_DIR = <prefix>/lib/cmake/NanoRos.
if(NOT DEFINED NanoRos_DIR)
    message(FATAL_ERROR
        "nuttx-support.cmake requires find_package(NanoRos CONFIG REQUIRED) "
        "to be called first.")
endif()
get_filename_component(_NROS_CPP_INCLUDE_DIR "${NanoRos_DIR}/../../../include" ABSOLUTE)

# ============================================================================
# nuttx_build_example() — builds a NuttX kernel ELF with C/C++ app
# ============================================================================
#
# Usage:
#   nuttx_build_example(<name> <main_cpp>
#       [INCLUDE_DIRS <dir1> <dir2> ...]
#       [SOURCES <extra_source1> <extra_source2> ...]
#       [COMPILE_DEFS <def1> <def2> ...]
#   )
#
# This function:
#   1. Runs `cargo +nightly build --release` on nros-nuttx-ffi
#   2. Sets APP_MAIN_CPP to the C/C++ source file
#   3. Sets APP_INCLUDE_DIRS to the codegen include directories
#   4. Sets APP_EXTRA_SOURCES to additional .c files (e.g. generated interfaces)
#   5. The resulting binary is at nros-nuttx-ffi/target/armv7a-nuttx-eabihf/release/nros-nuttx-ffi

function(nuttx_build_example name main_cpp)
    cmake_parse_arguments(_ARG "" "" "INCLUDE_DIRS;COMPILE_DEFS;SOURCES" ${ARGN})

    # Collect include dirs (nros-c/nros-cpp install headers + codegen output + user-specified)
    set(_include_dirs "${_NROS_CPP_INCLUDE_DIR}")
    foreach(_dir ${_ARG_INCLUDE_DIRS})
        list(APPEND _include_dirs "${_dir}")
    endforeach()
    string(JOIN ";" _include_dirs_str ${_include_dirs})

    # Collect extra source files (generated interfaces, etc.)
    set(_extra_sources "")
    foreach(_src ${_ARG_SOURCES})
        list(APPEND _extra_sources "${_src}")
    endforeach()
    string(JOIN ";" _extra_sources_str ${_extra_sources})

    # Collect compile definitions (from config.toml via nano_ros_read_config)
    set(_compile_defs "")
    foreach(_def ${_ARG_COMPILE_DEFS})
        list(APPEND _compile_defs "${_def}")
    endforeach()
    string(JOIN ";" _compile_defs_str ${_compile_defs})

    set(_output_binary "${_FFI_CRATE_DIR}/target/armv7a-nuttx-eabihf/release/nros-nuttx-ffi")

    add_custom_command(
        OUTPUT "${_output_binary}"
        COMMAND ${CMAKE_COMMAND} -E env
            "APP_MAIN_CPP=${main_cpp}"
            "APP_INCLUDE_DIRS=${_include_dirs_str}"
            "APP_EXTRA_SOURCES=${_extra_sources_str}"
            "APP_COMPILE_DEFS=${_compile_defs_str}"
            "NUTTX_DIR=${NUTTX_DIR}"
            "NUTTX_APPS_DIR=${NUTTX_DIR}/../nuttx-apps"
            cargo +nightly build --release
        WORKING_DIRECTORY "${_FFI_CRATE_DIR}"
        DEPENDS "${main_cpp}" ${_ARG_SOURCES}
        COMMENT "Building NuttX C++ example: ${name}"
        VERBATIM
    )

    add_custom_target(${name}_build ALL DEPENDS "${_output_binary}")

    # Copy the binary to the build directory for convenience
    add_custom_command(
        TARGET ${name}_build POST_BUILD
        COMMAND ${CMAKE_COMMAND} -E copy "${_output_binary}" "${CMAKE_CURRENT_BINARY_DIR}/${name}"
        COMMENT "Copying ${name} to build directory"
    )
endfunction()
