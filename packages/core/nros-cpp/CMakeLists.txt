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
# Phase 128.C.3 — every backend now routes through the same `rmw-cffi`
# Rust feature; the per-backend `rmw-*-cffi` features were deleted.
# Selection happens at link time via `target_link_libraries(...
# NanoRos::Rmw::<name>)`; this block only validates `NANO_ROS_RMW`
# spelling so configure errors out early when the user picks an
# unknown name.
if(NOT (NANO_ROS_RMW STREQUAL "zenoh" OR NANO_ROS_RMW STREQUAL "xrce"
        OR NANO_ROS_RMW STREQUAL "dds" OR NANO_ROS_RMW STREQUAL "cyclonedds"
        OR NANO_ROS_RMW STREQUAL "uorb"))
    message(FATAL_ERROR
        "Unknown NANO_ROS_RMW: ${NANO_ROS_RMW} "
        "(expected: zenoh, xrce, dds, cyclonedds, or uorb)")
endif()
set(_rmw_features rmw-cffi)

# ---- Map platform to Cargo features ----
if(NANO_ROS_PLATFORM STREQUAL "posix")
    set(_platform_features std platform-posix)
elseif(NANO_ROS_PLATFORM STREQUAL "freertos_armcm3"
       OR NANO_ROS_PLATFORM STREQUAL "freertos")
    # Phase 138 canonical "freertos" accepted alongside legacy
    # "freertos_armcm3"; Phase 138 collapsed board into NANO_ROS_BOARD.
    set(_platform_features alloc panic-halt platform-freertos)
elseif(NANO_ROS_PLATFORM STREQUAL "nuttx_armv7a"
       OR NANO_ROS_PLATFORM STREQUAL "nuttx")
    # Phase 138 canonical "nuttx" accepted alongside legacy
    # "nuttx_armv7a"; Phase 138 collapsed board into NANO_ROS_BOARD.
    set(_platform_features std platform-nuttx)
elseif(NANO_ROS_PLATFORM STREQUAL "threadx_linux")
    set(_platform_features std platform-threadx)
elseif(NANO_ROS_PLATFORM STREQUAL "threadx_riscv64")
    set(_platform_features alloc panic-halt platform-threadx)
elseif(NANO_ROS_PLATFORM STREQUAL "threadx")
    # Phase 138 canonical "threadx" — board axis disambiguates the
    # std-vs-no_std split (Linux sim has a host libc; RV64 QEMU is
    # bare-metal). Required for `add_subdirectory(<nano-ros-root>)`
    # consumers per Phase 144.7-8.
    if(NANO_ROS_BOARD STREQUAL "threadx-linux")
        set(_platform_features std platform-threadx)
    elseif(NANO_ROS_BOARD STREQUAL "riscv64-qemu")
        set(_platform_features alloc panic-halt platform-threadx)
    else()
        message(FATAL_ERROR
            "NANO_ROS_PLATFORM=threadx requires NANO_ROS_BOARD to be one of: "
            "threadx-linux, riscv64-qemu. Got '${NANO_ROS_BOARD}'.")
    endif()
else()
    message(FATAL_ERROR "Unknown NANO_ROS_PLATFORM: ${NANO_ROS_PLATFORM} "
            "(expected: posix, freertos_armcm3, freertos, nuttx_armv7a, "
            "threadx_linux, threadx_riscv64, or threadx)")
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

# Phase 144 — mirror the per-build `nros_cpp_config_generated.h` (produced
# by nros-cpp's build.rs into CMAKE_CURRENT_BINARY_DIR via corrosion) to
# a sibling `include/nros/` dir that we add to nros-cpp-headers' INTERFACE.
# Same pattern as the nros-c sub-project (see Phase 137 comment block in
# `packages/core/nros-c/CMakeLists.txt`). The mirrored copy is searched
# BEFORE the source-tree stub, so per-variant storage sizes take effect
# under add_subdirectory(<repo-root>) consumption.
set(_nros_cpp_intree_include_dir "${CMAKE_CURRENT_BINARY_DIR}/include")
file(MAKE_DIRECTORY "${_nros_cpp_intree_include_dir}/nros")
if(TARGET cargo-build_nros_cpp)
    add_custom_command(TARGET cargo-build_nros_cpp POST_BUILD
        COMMAND ${CMAKE_COMMAND} -E copy_if_different
            "${CMAKE_CURRENT_BINARY_DIR}/nros_cpp_config_generated.h"
            "${_nros_cpp_intree_include_dir}/nros/nros_cpp_config_generated.h"
        BYPRODUCTS "${_nros_cpp_intree_include_dir}/nros/nros_cpp_config_generated.h"
        COMMENT "nros-cpp: mirroring nros_cpp_config_generated.h for in-tree consumers"
        VERBATIM
    )
    target_include_directories(nros-cpp-headers BEFORE INTERFACE
        $<BUILD_INTERFACE:${_nros_cpp_intree_include_dir}>)
endif()

# Link against the FFI static library
target_link_libraries(nros-cpp-headers INTERFACE nros_cpp-static)
# Phase 144 — pull NanoRos::NanoRos into nros-cpp-headers transitively
# so consumers linking NanoRos::NanoRosCpp also get the C-side headers
# (`<nros/app_main.h>` etc.) reachable. Matches the legacy
# `NanoRosCppTargets.cmake` shape (`INTERFACE_LINK_LIBRARIES
# NanoRos::NanoRosCppLib NanoRos::NanoRos`). The root `CMakeLists.txt`
# declares `NanoRos::NanoRos` AFTER add_subdirectory'ing this file
# (it's the umbrella that wires in nros-c + the platform shim +
# the RMW staticlib), so we register the link via a deferred call
# rooted in the nano-ros root directory.
cmake_language(DEFER DIRECTORY "${CMAKE_CURRENT_LIST_DIR}/../../.." CALL
    target_link_libraries nros-cpp-headers INTERFACE NanoRos::NanoRos)

# Phase 117.8: when the consumer asks for the Cyclone DDS RMW backend,
# pull in the standalone `nros-rmw-cyclonedds` library + Cyclone DDS
# itself transitively, and flip on the auto-register call inside
# `nros::init` via the `NROS_RMW_CYCLONEDDS` macro.
if(NANO_ROS_RMW STREQUAL "cyclonedds")
    find_package(NrosRmwCyclonedds CONFIG REQUIRED)
    target_link_libraries(nros-cpp-headers INTERFACE
        NrosRmwCyclonedds::NrosRmwCyclonedds)
    target_compile_definitions(nros-cpp-headers INTERFACE NROS_RMW_CYCLONEDDS=1)
elseif(NANO_ROS_RMW STREQUAL "xrce")
    # Phase 144.3 — XRCE via the `nros-rmw-xrce-cffi` Rust shim, which
    # is built+linked by the root `CMakeLists.txt` xrce branch (see
    # add_subdirectory path). Mirrors the dds/zenoh shape above: no
    # find_package required — the `nros_rmw_xrce_register()` symbol
    # lives in the staticlib and auto-registers via the `linkme`
    # distributed-slice (Phase 128.B.3). The compile define is kept
    # under the `_CFFI` suffix to distinguish from the legacy
    # `NROS_RMW_XRCE` define still emitted by `NanoRosCppTargets.cmake`
    # for installed find_package consumers.
    target_compile_definitions(nros-cpp-headers INTERFACE NROS_RMW_XRCE_CFFI=1)
elseif(NANO_ROS_RMW STREQUAL "dds")
    # Phase 115.L.1: dust-DDS via the C vtable. The register
    # entry point `nros_rmw_dds_register()` ships from the
    # Rust-side `nros-rmw-dds` shim crate (pulled via the
    # `rmw-dds-cffi` Cargo feature), so no extra find_package
    # is required — the symbol is in the staticlib produced by
    # corrosion_import_crate above.
    target_compile_definitions(nros-cpp-headers INTERFACE NROS_RMW_DDS_CFFI=1)
elseif(NANO_ROS_RMW STREQUAL "zenoh")
    # Phase 115.L.2: zenoh-pico via the C vtable. Same shape as
    # the `dds` path above — `nros_rmw_zenoh_register()` lives
    # in the Rust shim.
    target_compile_definitions(nros-cpp-headers INTERFACE NROS_RMW_ZENOH_CFFI=1)
elseif(NANO_ROS_RMW STREQUAL "uorb")
    # Phase 115.K.4: PX4 uORB backend (C++ port). Mirrors
    # cyclonedds shape — find_package + link + auto-register
    # via the `NROS_RMW_UORB` define in nros::init.
    find_package(NrosRmwUorb CONFIG REQUIRED)
    target_link_libraries(nros-cpp-headers INTERFACE
        NrosRmwUorb::NrosRmwUorb)
    target_compile_definitions(nros-cpp-headers INTERFACE NROS_RMW_UORB=1)
endif()

# Require at least C++14
target_compile_features(nros-cpp-headers INTERFACE cxx_std_14)

# ---- Install ----
install(DIRECTORY "${CMAKE_CURRENT_SOURCE_DIR}/include/nros"
    DESTINATION include
    FILES_MATCHING PATTERN "*.hpp" PATTERN "*.h"
)

# Phase 119.2: install variant-specific generated header to a variant
# subdir so each cmake build's exact storage sizes survive alongside
# every other variant. The source-tree `nros_cpp_config_generated.h`
# (max-merged upper bound for direct cargo users) is also installed
# above as a fallback.
#
# nros-cpp's build.rs writes the per-build header to
# `${CORROSION_BUILD_DIR}/nros_cpp_config_generated.h` (== this
# CMakeLists.txt's CMAKE_CURRENT_BINARY_DIR — corrosion sets that env
# var to point here).
# Phase 128.C.4 — the Cargo build is RMW-agnostic now (only
# `rmw-cffi` is fed through), so the install paths drop the RMW
# suffix. Backend selection happens at the consumer's link step via
# `target_link_libraries(... NanoRos::Rmw::<name>)`.
if(NANO_ROS_PLATFORM STREQUAL "posix")
    set(_cpp_variant_subdir "nros_cpp")
else()
    set(_cpp_variant_subdir "nros_cpp_${NANO_ROS_PLATFORM}")
endif()
install(FILES "${CMAKE_CURRENT_BINARY_DIR}/nros_cpp_config_generated.h"
    DESTINATION "include/${_cpp_variant_subdir}/nros"
    OPTIONAL
)

if(NANO_ROS_PLATFORM STREQUAL "posix")
    set(_cpp_lib_name "libnros_cpp.a")
else()
    set(_cpp_lib_name "libnros_cpp_${NANO_ROS_PLATFORM}.a")
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
