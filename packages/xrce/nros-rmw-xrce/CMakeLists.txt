cmake_minimum_required(VERSION 3.16)
project(nros_rmw_xrce
    VERSION 0.1.0
    DESCRIPTION "micro-XRCE-DDS-Client RMW backend for nano-ros (Phase 115.K.2)"
    LANGUAGES C
)

# ---- C standard ------------------------------------------------------------
# Pure C99 — micro-XRCE-DDS-Client itself is C99, and the nros-rmw-cffi
# C ABI is C99. Stay portable so this scaffold can ship into Zephyr /
# FreeRTOS / NuttX targets without bumping the toolchain requirement.
set(CMAKE_C_STANDARD 99)
set(CMAKE_C_STANDARD_REQUIRED ON)
set(CMAKE_C_EXTENSIONS OFF)

include(GNUInstallDirs)
include(CMakePackageConfigHelpers)

# ---- Dependencies ----------------------------------------------------------
# nros-rmw-cffi C ABI vtable + return codes + entity structs. Headers
# only — the backend implements the vtable and registers it via
# `nros_rmw_cffi_register()`. Path is exposed via cache var so callers
# (or `find_package(NrosRmwXrce)`) can override it for out-of-tree
# builds. Mirrors `nros-rmw-cyclonedds`'s convention.
set(NROS_RMW_CFFI_DIR "${CMAKE_CURRENT_SOURCE_DIR}/../../core/nros-rmw-cffi/include"
    CACHE PATH "Path to the nros-rmw-cffi public headers (rmw_vtable.h, rmw_ret.h, …)")
if(NOT EXISTS "${NROS_RMW_CFFI_DIR}/nros/rmw_vtable.h")
    message(FATAL_ERROR
        "nros-rmw-cffi headers not found at NROS_RMW_CFFI_DIR='${NROS_RMW_CFFI_DIR}'. "
        "Set -DNROS_RMW_CFFI_DIR=<path-to-packages/core/nros-rmw-cffi/include>.")
endif()

set(NROS_PLATFORM_CFFI_INCLUDE "${CMAKE_CURRENT_SOURCE_DIR}/../../core/nros-platform-api/include"
    CACHE PATH "Path to the nros-platform-cffi public headers (platform.h, platform_net.h, …)")
if(NOT EXISTS "${NROS_PLATFORM_CFFI_INCLUDE}/nros/platform_net.h")
    message(FATAL_ERROR
        "nros-platform-cffi headers not found at NROS_PLATFORM_CFFI_INCLUDE='${NROS_PLATFORM_CFFI_INCLUDE}'. "
        "Set -DNROS_PLATFORM_CFFI_INCLUDE=<path-to-packages/core/nros-platform-api/include>.")
endif()

# micro-XRCE-DDS-Client + micro-CDR (vendored under xrce-sys). Phase
# 115.K.2.1 compiles their sources directly into nros_rmw_xrce
# rather than going through the upstream CMakeLists' add_subdirectory
# / find_package machinery. The upstream client's `find_package(microcdr
# REQUIRED CONFIG)` requires a separate install step that buys nothing
# in our in-tree case; xrce-sys's `build.rs` already proved the
# inline-compile approach (same source list mirrored below).
set(MICROXRCEDDS_CLIENT_DIR
    "${CMAKE_CURRENT_SOURCE_DIR}/../xrce-sys/micro-xrce-dds-client"
    CACHE PATH "Path to vendored micro-xrce-dds-client checkout")
set(MICROCDR_DIR
    "${CMAKE_CURRENT_SOURCE_DIR}/../xrce-sys/micro-cdr"
    CACHE PATH "Path to vendored micro-cdr checkout")

# Generate `<uxr/client/config.h>` from the upstream template. Values
# match xrce-sys's `posix_compat = true, mtu = 4096` defaults; further
# tuning lands when 115.K.2.5 flips the C backend on under
# `-DNROS_C_RMW=xrce`.
set(NROS_RMW_XRCE_C_PROJECT_VERSION_MAJOR 2)
set(NROS_RMW_XRCE_C_PROJECT_VERSION_MINOR 4)
set(NROS_RMW_XRCE_C_PROJECT_VERSION_PATCH 1)
set(NROS_RMW_XRCE_C_PROJECT_VERSION 2.4.1)

set(_uxr_config_in   "${MICROXRCEDDS_CLIENT_DIR}/include/uxr/client/config.h.in")
set(_uxr_config_out  "${CMAKE_CURRENT_BINARY_DIR}/uxr_generated/uxr/client/config.h")

# Map the @TOKEN@ placeholders the template expects to our chosen
# values. `set(... PARENT_SCOPE)` not needed — configure_file reads
# the current scope.
set(PROJECT_VERSION_MAJOR  ${NROS_RMW_XRCE_C_PROJECT_VERSION_MAJOR})
set(PROJECT_VERSION_MINOR  ${NROS_RMW_XRCE_C_PROJECT_VERSION_MINOR})
set(PROJECT_VERSION_PATCH  ${NROS_RMW_XRCE_C_PROJECT_VERSION_PATCH})
set(PROJECT_VERSION        ${NROS_RMW_XRCE_C_PROJECT_VERSION})
set(UCLIENT_MAX_OUTPUT_BEST_EFFORT_STREAMS 1)
set(UCLIENT_MAX_OUTPUT_RELIABLE_STREAMS    1)
set(UCLIENT_MAX_INPUT_BEST_EFFORT_STREAMS  1)
set(UCLIENT_MAX_INPUT_RELIABLE_STREAMS     1)
set(UCLIENT_MAX_SESSION_CONNECTION_ATTEMPTS 10)
set(UCLIENT_MIN_SESSION_CONNECTION_INTERVAL 1000)
set(UCLIENT_MIN_HEARTBEAT_TIME_INTERVAL    100)
set(UCLIENT_UDP_TRANSPORT_MTU              4096)
set(UCLIENT_TCP_TRANSPORT_MTU              4096)
set(UCLIENT_SERIAL_TRANSPORT_MTU           512)
set(UCLIENT_CUSTOM_TRANSPORT_MTU           4096)
set(UCLIENT_SHARED_MEMORY_MAX_ENTITIES     4)
set(UCLIENT_SHARED_MEMORY_STATIC_MEM_SIZE  10)
set(UCLIENT_HARD_LIVELINESS_CHECK_TIMEOUT  10000)
set(UCLIENT_TWEAK_XRCE_WRITE_LIMIT         1)

# Upstream template uses #cmakedefine — every feature toggle becomes
# either `#define` or `/* #undef */`. Set vars to true/empty.
set(UCLIENT_PROFILE_DISCOVERY              TRUE)
set(UCLIENT_PROFILE_UDP                    TRUE)
set(UCLIENT_PROFILE_TCP                    TRUE)
set(UCLIENT_PROFILE_SERIAL                 TRUE)
set(UCLIENT_PROFILE_CUSTOM_TRANSPORT       TRUE)
set(UCLIENT_PROFILE_STREAM_FRAMING         TRUE)
set(UCLIENT_PROFILE_MULTITHREAD)
set(UCLIENT_PROFILE_SHARED_MEMORY)
set(UCLIENT_PROFILE_MATCHING)
set(UCLIENT_PROFILE_CAN)
set(UCLIENT_BIG_ENDIANNESS)
set(UCLIENT_HARD_LIVELINESS_CHECK)
set(UCLIENT_TWEAK_XRCE_WRITE_LIMIT_FLAG    TRUE)
set(UCLIENT_PLATFORM_POSIX                 TRUE)
set(UCLIENT_PLATFORM_POSIX_NOPOLL)
set(UCLIENT_PLATFORM_WINDOWS)
set(UCLIENT_PLATFORM_FREERTOS_PLUS_TCP)
set(UCLIENT_PLATFORM_RTEMS_BSD_NET)
set(UCLIENT_PLATFORM_ZEPHYR)

configure_file("${_uxr_config_in}" "${_uxr_config_out}" @ONLY)

# micro-CDR config.h — same configure_file pattern.
set(CONFIG_MACHINE_ENDIANNESS 1)  # ucdrEndianness: BIG=0, LITTLE=1 (x86 / ARM common case)
configure_file(
    "${MICROCDR_DIR}/include/ucdr/config.h.in"
    "${CMAKE_CURRENT_BINARY_DIR}/ucdr_generated/ucdr/config.h"
    @ONLY)

# Source lists mirror packages/xrce/xrce-sys/build.rs (proven
# working set). Keeping these in lockstep is a 115.K.2 invariant —
# if the Rust build.rs adds a file, this CMakeLists must add it too,
# and vice versa.
set(_uxr_session_src
    core/session/session.c
    core/session/session_info.c
    core/session/submessage.c
    core/session/object_id.c
    core/session/common_create_entities.c
    core/session/create_entities_bin.c
    core/session/create_entities_ref.c
    core/session/create_entities_xml.c
    core/session/read_access.c
    core/session/write_access.c
)
set(_uxr_stream_src
    core/session/stream/input_best_effort_stream.c
    core/session/stream/input_reliable_stream.c
    core/session/stream/output_best_effort_stream.c
    core/session/stream/output_reliable_stream.c
    core/session/stream/stream_storage.c
    core/session/stream/stream_id.c
    core/session/stream/seq_num.c
)
set(_uxr_serialization_src
    core/serialization/xrce_types.c
    core/serialization/xrce_header.c
    core/serialization/xrce_subheader.c
)
set(_uxr_misc_src
    util/ping.c
    util/time.c
    profile/transport/custom/custom_transport.c
    profile/transport/stream_framing/stream_framing_protocol.c
    profile/transport/ip/udp/udp_transport.c
    profile/transport/ip/udp/udp_transport_posix.c
)
set(_uxr_all_src "")
foreach(_rel IN LISTS _uxr_session_src _uxr_stream_src _uxr_serialization_src _uxr_misc_src)
    list(APPEND _uxr_all_src "${MICROXRCEDDS_CLIENT_DIR}/src/c/${_rel}")
endforeach()

set(_ucdr_src
    common.c
    types/basic.c
    types/array.c
    types/sequence.c
    types/string.c
)
set(_ucdr_all_src "")
foreach(_rel IN LISTS _ucdr_src)
    list(APPEND _ucdr_all_src "${MICROCDR_DIR}/src/c/${_rel}")
endforeach()

# ---- Static library --------------------------------------------------------
add_library(nros_rmw_xrce STATIC
    src/vtable.c
    src/session.c
    src/publisher.c
    src/subscriber.c
    src/service.c
    src/transport_custom.c
    src/transport_nros_udp.c
    src/transport_posix_udp.c
    src/transport_posix_serial.c
    src/platform_aliases.c
    ${_uxr_all_src}
    ${_ucdr_all_src}
)

target_include_directories(nros_rmw_xrce
    PUBLIC
        $<BUILD_INTERFACE:${CMAKE_CURRENT_SOURCE_DIR}/include>
        $<INSTALL_INTERFACE:${CMAKE_INSTALL_INCLUDEDIR}>
    PRIVATE
        ${NROS_RMW_CFFI_DIR}
        ${NROS_PLATFORM_CFFI_INCLUDE}
        ${CMAKE_CURRENT_SOURCE_DIR}/src
        ${MICROXRCEDDS_CLIENT_DIR}/include
        ${MICROXRCEDDS_CLIENT_DIR}/src/c
        ${MICROCDR_DIR}/include
        ${CMAKE_CURRENT_BINARY_DIR}/uxr_generated
        ${CMAKE_CURRENT_BINARY_DIR}/ucdr_generated
)

target_compile_definitions(nros_rmw_xrce PRIVATE
    # POSIX feature gate — `time.c` calls `clock_gettime`, which
    # only resolves under `_POSIX_C_SOURCE >= 199309L`. Match
    # xrce-sys's build environment.
    _POSIX_C_SOURCE=200809L
    _DEFAULT_SOURCE
)

target_compile_options(nros_rmw_xrce PRIVATE
    -Wall -Wextra -Wpedantic
    # Vendored upstream sources trip strictness our own code wants;
    # quiet just enough to keep the lint bar useful for our TUs.
    -Wno-unused-parameter
    -Wno-pedantic
)

set_target_properties(nros_rmw_xrce PROPERTIES
    POSITION_INDEPENDENT_CODE ON
    EXPORT_NAME NrosRmwXrce
)

# Phase 140 — install rules + Config.cmake.in template deleted.
# `nros-rmw-xrce` (the K.2 header-only C++ backend) is no longer
# wired through the root CMake — XRCE consumption goes through the
# Rust-side `nros-rmw-xrce-cffi` shim, added via Corrosion in the
# root's xrce branch. This CMakeLists builds the C++ backend for
# in-tree CTest harnesses; it's not part of the user-facing surface.

# ---- Tests -----------------------------------------------------------------
option(NROS_RMW_XRCE_C_BUILD_TESTS "Build CTest harness" ON)
if(NROS_RMW_XRCE_C_BUILD_TESTS)
    enable_testing()
    add_subdirectory(tests)
endif()
