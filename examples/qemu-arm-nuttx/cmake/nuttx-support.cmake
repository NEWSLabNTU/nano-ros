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

# Cross-compile target for any cargo invocation triggered downstream
# (notably the per-package `nano_ros_cpp_ffi_<pkg>` static libs the
# codegen pipeline builds in `nros_find_interfaces`). Without this the
# FFI staticlibs get built for the host triple and ld fails with
# `file format not recognized` when the leaf NuttX ELF tries to link
# them. The codegen cmake's nuttx branch (NanoRosGenerateInterfaces.cmake
# line ~391) already handles `Rust_CARGO_TARGET MATCHES "nuttx"` by
# emitting a `.cargo/config.toml` with `build-std=core` + the nightly
# toolchain prefix, so just setting the variable is enough.
if(NOT DEFINED Rust_CARGO_TARGET)
    set(Rust_CARGO_TARGET "armv7a-nuttx-eabihf")
endif()

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
#   1. Runs `cargo build --release` on nros-nuttx-ffi (the crate's
#      rust-toolchain.toml pins the required nightly channel)
#   2. Sets APP_MAIN_CPP to the C/C++ source file
#   3. Sets APP_INCLUDE_DIRS to the codegen include directories
#   4. Sets APP_EXTRA_SOURCES to additional .c files (e.g. generated interfaces)
#   5. The resulting binary is at nros-nuttx-ffi/target/armv7a-nuttx-eabihf/release/nros-nuttx-ffi

function(nuttx_build_example name main_cpp)
    cmake_parse_arguments(_ARG "" ""
        "INCLUDE_DIRS;COMPILE_DEFS;SOURCES;LINK_INTERFACES"
        ${ARGN})

    # Collect static include dirs (nros-c/nros-cpp install headers + user
    # supplied). Each LINK_INTERFACES library's INTERFACE_INCLUDE_DIRECTORIES
    # is a generator expression that resolves to a list, which doesn't
    # round-trip through `cmake -E env` cleanly (semicolons either explode
    # into separate args, or pass through verbatim and confuse cargo). Use
    # `file(GENERATE)` to materialise the closure to a sentinel file and
    # let build.rs read it. cmake walks the INTERFACE_LINK_LIBRARIES graph
    # transitively, so each leaf `<pkg>__nano_ros_cpp` library's include
    # closure (umbrella header dir for every transitive dep package) flows
    # in automatically.
    set(_static_includes "${_NROS_CPP_INCLUDE_DIR}")
    foreach(_dir ${_ARG_INCLUDE_DIRS})
        list(APPEND _static_includes "${_dir}")
    endforeach()
    set(_includes_file "${CMAKE_CURRENT_BINARY_DIR}/${name}_includes.txt")
    set(_iface_genex_lines "")
    foreach(_lib ${_ARG_LINK_INTERFACES})
        list(APPEND _iface_genex_lines
            "$<JOIN:$<TARGET_PROPERTY:${_lib},INTERFACE_INCLUDE_DIRECTORIES>,\n>")
    endforeach()
    set(_static_block "")
    foreach(_dir ${_static_includes})
        string(APPEND _static_block "${_dir}\n")
    endforeach()
    file(GENERATE
        OUTPUT "${_includes_file}"
        CONTENT "${_static_block}$<JOIN:${_iface_genex_lines},\n>\n")

    # FFI static libraries to link: only the leaf packages in
    # `LINK_INTERFACES`. The codegen pipeline builds one
    # `<leaf>__nano_ros_cpp_ffi_lib` per package, and that crate's
    # `lib.rs` `include!()`s the FFI Rust glue from every transitive
    # dep (see `NanoRosGenerateInterfaces.cmake` line ~362). So linking
    # the leaves transitively pulls in all dep types — and the dep
    # packages' own `*_ffi_lib` static libs aren't built (and aren't
    # needed). Only `<pkg>__nano_ros_cpp_gen` runs for each transitive
    # dep so the codegen .hpp / .rs files exist for `include!()`.
    set(_ffi_libs_file "${CMAKE_CURRENT_BINARY_DIR}/${name}_ffi_libs.txt")
    set(_ffi_lib_lines "")
    foreach(_lib ${_ARG_LINK_INTERFACES})
        if(TARGET ${_lib}_ffi_lib)
            list(APPEND _ffi_lib_lines
                "$<TARGET_FILE:${_lib}_ffi_lib>")
        endif()
    endforeach()
    if(_ffi_lib_lines)
        file(GENERATE
            OUTPUT "${_ffi_libs_file}"
            CONTENT "$<JOIN:${_ffi_lib_lines},\n>\n")
    else()
        file(GENERATE OUTPUT "${_ffi_libs_file}" CONTENT "")
    endif()

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

    # Per-example cargo target directory. Without this every example's
    # cargo build would land at the same
    # `${_FFI_CRATE_DIR}/target/armv7a-nuttx-eabihf/release/nros-nuttx-ffi`
    # path, and concurrent / sequential builds from different examples
    # silently clobber each other — the per-example `build/<name>`
    # POST_BUILD copy then ends up holding whichever example's main
    # cargo last linked. Surfaces as cross-binary contamination
    # (pubsub test launches the action-server image, etc.). Cargo
    # honours `CARGO_TARGET_DIR` and the codegen FFI staticlibs
    # already use their own per-package target dirs, so this only
    # affects the leaf NuttX binary.
    set(_cargo_target_dir "${CMAKE_CURRENT_BINARY_DIR}/cargo-target")
    set(_output_binary "${_cargo_target_dir}/armv7a-nuttx-eabihf/release/nros-nuttx-ffi")

    add_custom_command(
        OUTPUT "${_output_binary}"
        COMMAND ${CMAKE_COMMAND} -E env
            "APP_MAIN_CPP=${main_cpp}"
            "APP_INCLUDE_DIRS_FILE=${_includes_file}"
            "APP_FFI_LIBS_FILE=${_ffi_libs_file}"
            "APP_EXTRA_SOURCES=${_extra_sources_str}"
            "APP_COMPILE_DEFS=${_compile_defs_str}"
            "NUTTX_DIR=${NUTTX_DIR}"
            "NUTTX_APPS_DIR=${NUTTX_DIR}/../nuttx-apps"
            "CARGO_TARGET_DIR=${_cargo_target_dir}"
            cargo build --release
        WORKING_DIRECTORY "${_FFI_CRATE_DIR}"
        DEPENDS "${main_cpp}" ${_ARG_SOURCES} "${_includes_file}" "${_ffi_libs_file}"
        COMMENT "Building NuttX C++ example: ${name}"
        VERBATIM
    )

    add_custom_target(${name}_build ALL DEPENDS "${_output_binary}")

    # Pull in each interface library's transitive dependency closure: the
    # cmake link graph already records `<pkg>__nano_ros_cpp` → its
    # transitive `_gen` codegen targets (every dep package's umbrella +
    # leaf headers), so a single add_dependencies on each leaf interface
    # lib chains the whole codegen DAG before cargo runs.
    foreach(_lib ${_ARG_LINK_INTERFACES})
        add_dependencies(${name}_build ${_lib})
    endforeach()

    # Copy the binary to the build directory for convenience
    add_custom_command(
        TARGET ${name}_build POST_BUILD
        COMMAND ${CMAKE_COMMAND} -E copy "${_output_binary}" "${CMAKE_CURRENT_BINARY_DIR}/${name}"
        COMMENT "Copying ${name} to build directory"
    )
endfunction()
