# nuttx-support.cmake
#
# Layer-3 cmake support module for NuttX C/C++ examples (QEMU ARM virt).
# Phase 112.E: shipped via `find_package(NanoRos)` install layout.
#
# Unlike FreeRTOS / ThreadX, NuttX uses its own native build system
# (kconfig + make). cmake's job here is **not** to rebuild the
# kernel — it's to drive `cargo build` on `nros-nuttx-ffi` (a Rust
# crate whose build.rs invokes the NuttX toolchain on the user's
# main.c/main.cpp + codegen sources, and links against pre-built
# NuttX libs from NUTTX_DIR / NUTTX_APPS_DIR).
#
# Provides:
#   nuttx_build_example(<name> <main_cpp>
#       [INCLUDE_DIRS …] [SOURCES …] [COMPILE_DEFS …] [LINK_INTERFACES …])
#
# Caller must already have done:
#   find_package(NanoRos CONFIG REQUIRED)
#   include(nuttx-support)

include(nros-nuttx)

nros_nuttx_validate(REQUIRE NanoRos_DIR)
nros_nuttx_set_cargo_target("armv7a-nuttx-eabihf")

# FFI crate ships under share/nano_ros/platform/nuttx/.
get_filename_component(_NUTTX_SUPPORT_DIR "${CMAKE_CURRENT_LIST_FILE}" DIRECTORY)
get_filename_component(_NROS_INSTALL_PREFIX "${_NUTTX_SUPPORT_DIR}/../../.." ABSOLUTE)
set(_NUTTX_FFI_CRATE_DIR
    "${_NROS_INSTALL_PREFIX}/share/nano_ros/platform/nuttx/nros-nuttx-ffi")
if(NOT EXISTS "${_NUTTX_FFI_CRATE_DIR}/Cargo.toml")
    message(FATAL_ERROR
        "nuttx-support: nros-nuttx-ffi crate not found at ${_NUTTX_FFI_CRATE_DIR}. "
        "Reinstall NanoRos (`just nuttx install`).")
endif()

# Backward-compat wrapper. Existing per-example CMakeLists.txt files
# call `nuttx_build_example(<name> <main> ...)` (positional name +
# main, then keyword args). Forward to the layer-2 keyword-only
# function with the platform-specific defaults filled in.
function(nuttx_build_example name main_cpp)
    nros_nuttx_build_example(
        NAME            "${name}"
        MAIN_SOURCE     "${main_cpp}"
        FFI_CRATE_DIR   "${_NUTTX_FFI_CRATE_DIR}"
        TARGET_TRIPLE   "armv7a-nuttx-eabihf"
        ${ARGN})
endfunction()
