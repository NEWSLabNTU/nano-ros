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

# FFI crate path. Phase 112.E.deferred — NuttX FFI crate has nested
# path deps on workspace board crates that would dangle if relocated
# under <prefix>/share/. Until a workspace-flattening installer
# lands, examples pass `-DNUTTX_FFI_CRATE_DIR=<repo>/examples/qemu-arm-nuttx/cmake/nros-nuttx-ffi`
# (or set the env var) so this support file can locate the in-tree
# crate.
if(NOT DEFINED NUTTX_FFI_CRATE_DIR AND DEFINED ENV{NUTTX_FFI_CRATE_DIR})
    set(NUTTX_FFI_CRATE_DIR "$ENV{NUTTX_FFI_CRATE_DIR}")
endif()
if(NOT NUTTX_FFI_CRATE_DIR OR NOT EXISTS "${NUTTX_FFI_CRATE_DIR}/Cargo.toml")
    message(FATAL_ERROR
        "nuttx-support: NUTTX_FFI_CRATE_DIR not set or invalid. Pass "
        "-DNUTTX_FFI_CRATE_DIR=<path>/nros-nuttx-ffi (or export the env var). "
        "In-tree path: <repo>/examples/qemu-arm-nuttx/cmake/nros-nuttx-ffi.")
endif()
set(_NUTTX_FFI_CRATE_DIR "${NUTTX_FFI_CRATE_DIR}")

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
