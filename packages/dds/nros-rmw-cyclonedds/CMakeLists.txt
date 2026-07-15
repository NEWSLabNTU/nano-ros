cmake_minimum_required(VERSION 3.16)
project(nros_rmw_cyclonedds
    VERSION 0.1.0
    DESCRIPTION "Cyclone DDS RMW backend for nano-ros (Phase 117)"
    LANGUAGES C CXX
)

# ---- RPATH -----------------------------------------------------------------
# Test binaries link against `libddsc.so` in the project's
# `build/install/lib`; without an rpath the dynamic linker finds the
# system-installed `ros-humble-cyclonedds` first, which is ABI-
# incompatible with the in-tree Cyclone build (our pin is 0.10.5
# but the build flags differ).
set(CMAKE_BUILD_RPATH_USE_ORIGIN ON)
set(CMAKE_INSTALL_RPATH_USE_LINK_PATH ON)
list(APPEND CMAKE_BUILD_RPATH "${CMAKE_PREFIX_PATH}/lib")

# ---- C++ standard ----------------------------------------------------------
# nano-ros core C++ surface is C++14 freestanding. The Cyclone-side wrapper
# stays at the same baseline so it can ship into Zephyr / FreeRTOS targets
# without bumping the toolchain requirement.
set(CMAKE_CXX_STANDARD 14)
set(CMAKE_CXX_STANDARD_REQUIRED ON)
set(CMAKE_CXX_EXTENSIONS OFF)

# Phase 140 — install rules + Config.cmake.in template deleted.
# Consumed in-tree via `add_subdirectory(packages/dds/nros-rmw-cyclonedds)`
# from the root `CMakeLists.txt` cyclonedds branch. CycloneDDS itself
# is still a third-party dependency surfaced via find_package(CycloneDDS)
# — callers point CMAKE_PREFIX_PATH at its install prefix.

# ---- Dependencies ----------------------------------------------------------
# Cyclone DDS itself. Phase 186 — resolved via nros_provide_cyclonedds():
#   prebuilt install (find_package, on CMAKE_PREFIX_PATH / CycloneDDS_DIR) →
#   else self-provision from source (CYCLONEDDS_SOURCE_DIR; the project root
#   defaults it to the pinned third-party submodule, sccache-accelerated).
# So a bare cmake build needs no `just cyclonedds` pre-step. Sets
# NROS_CYCLONEDDS_PROVENANCE (target|find_package|source).
include("${CMAKE_CURRENT_SOURCE_DIR}/cmake/ProvideCycloneDDS.cmake")
nros_provide_cyclonedds()

# nros-rmw-cffi C ABI vtable + return codes + entity structs. Headers
# only — the backend implements the vtable and registers it via
# `nros_rmw_cffi_register()`. Path is exposed via cache var so callers
# (or `find_package(NrosRmwCyclonedds)`) can override it for out-of-tree
# builds.
set(NROS_RMW_CFFI_DIR "${CMAKE_CURRENT_SOURCE_DIR}/../../core/nros-rmw-cffi/include"
    CACHE PATH "Path to the nros-rmw-cffi public headers (rmw_vtable.h, rmw_ret.h, …)")
if(NOT EXISTS "${NROS_RMW_CFFI_DIR}/nros/rmw_vtable.h")
    message(FATAL_ERROR
        "nros-rmw-cffi headers not found at NROS_RMW_CFFI_DIR='${NROS_RMW_CFFI_DIR}'. "
        "Set -DNROS_RMW_CFFI_DIR=<path-to-packages/core/nros-rmw-cffi/include>.")
endif()

# ---- Static library --------------------------------------------------------
# Type-support helper (idlc + register-TU codegen). Loaded from the
# source tree at configure time so it's available before the install
# step runs.
include("${CMAKE_CURRENT_SOURCE_DIR}/cmake/NrosRmwCycloneddsTypeSupport.cmake")

# Phase 177.36 — bake the rmw_dds_common ParticipantEntitiesInfo descriptor into
# the backend so every Cyclone node can publish `ros_discovery_info` (the ROS 2
# node→endpoint graph stock tooling + action `wait_for_server` read). The
# hand-mangled IDL (src/idl/rmw_dds_common_graph.idl) yields the exact wire type
# `rmw_dds_common::msg::dds_::ParticipantEntitiesInfo_`. graph.cpp calls the
# generated `register_rmw_dds_common_graph_0()` explicitly so the descriptor TU
# is linked (no whole-archive needed).
set(_NROS_GRAPH_TYPES_DIR "${CMAKE_CURRENT_BINARY_DIR}/graph-types")
nros_rmw_cyclonedds_idlc_compile(_NROS_GRAPH_DESC_SRCS
    IDL_FILE   "${CMAKE_CURRENT_SOURCE_DIR}/src/idl/rmw_dds_common_graph.idl"
    OUTPUT_DIR "${_NROS_GRAPH_TYPES_DIR}"
    TYPE_NAME  "rmw_dds_common::msg::dds_::ParticipantEntitiesInfo_")

# Phase 117.X.3: service request-id correlation now uses an in-payload
# `cdds_request_header_t` (writer_guid[16] + sequence_number) injected
# at IDL-codegen time by `msg_to_cyclone_idl.py` for every `.srv`
# input. The backend-defined ServiceEnvelope IDL from 117.7.B is no
# longer needed and has been removed.

add_library(nros_rmw_cyclonedds STATIC
    src/vtable.cpp
    src/session.cpp
    src/publisher.cpp
    src/subscriber.cpp
    src/service.cpp
    src/descriptors.cpp
    src/graph.cpp
    src/qos.cpp
    src/sertype_min.cpp
    # Phase 212.K.7.4 — C++ bridge for the Rust DescriptorBuilder.
    # Currently a stub returning UnsupportedFieldType until the
    # Cyclone DDS pin advances to a release exposing the public
    # `ddsi_dynamic_type_*` API; the Rust ABI is stable so only this
    # TU's body needs to change at that point.
    bridge/dynamic_type_builder.cpp
    ${_NROS_GRAPH_DESC_SRCS}
)
add_library(NrosRmwCyclonedds::NrosRmwCyclonedds ALIAS nros_rmw_cyclonedds)

target_include_directories(nros_rmw_cyclonedds
    PUBLIC
        $<BUILD_INTERFACE:${CMAKE_CURRENT_SOURCE_DIR}/include>
        $<INSTALL_INTERFACE:${CMAKE_INSTALL_INCLUDEDIR}>
    PRIVATE
        ${NROS_RMW_CFFI_DIR}
        ${CMAKE_CURRENT_SOURCE_DIR}/src
        ${_NROS_GRAPH_TYPES_DIR}
)

target_link_libraries(nros_rmw_cyclonedds PUBLIC CycloneDDS::ddsc)

# Phase 186 — on the self-provision (source) path, CycloneDDS::ddsc is an
# add_subdirectory target that interface-exports only Cyclone's *public* headers.
# The wrapper (sertype_min.hpp) compiles against a few semi-internal ddsi/ddsrt
# headers that the *install* bundles but the build-tree target does not export
# (dds/ddsi/ddsi_cdrstream.h, q_protocol.h, ddsi_serdata_default.h, ddsi_sertype.h,
# plus generated dds/config.h + version.h). Add the source + generated roots
# directly. The find_package path already gets them from the install include dir.
if(NROS_CYCLONEDDS_PROVENANCE STREQUAL "source")
    target_include_directories(nros_rmw_cyclonedds PRIVATE
        "${CYCLONEDDS_SOURCE_DIR}/src/core/ddsi/include"
        "${CYCLONEDDS_SOURCE_DIR}/src/core/ddsc/include"
        "${CYCLONEDDS_SOURCE_DIR}/src/ddsrt/include"
        "${NROS_CYCLONEDDS_SOURCE_BUILD_DIR}/src/ddsrt/include")
    # The root CMakeLists whole-archives the backend together with
    # ${NROS_RMW_CYCLONEDDS_DDSC_LIBRARY}; on the find_package path that's the
    # imported libddsc, set by the rpath block below. On the source path ddsc is
    # an in-tree static target, so point the var at its build artifact (a genex,
    # evaluated when the root expands the link flag) — otherwise the final link
    # drops every dds_*/ddsrt_* symbol. Phase 186.
    set(NROS_RMW_CYCLONEDDS_DDSC_LIBRARY "$<TARGET_FILE:CycloneDDS::ddsc>"
        CACHE STRING "Resolved Cyclone DDS ddsc library (source path)" FORCE)
endif()

if(DEFINED NANO_ROS_PLATFORM
   AND (NANO_ROS_PLATFORM STREQUAL "freertos"
        OR NANO_ROS_PLATFORM STREQUAL "freertos_armcm3"))
    target_compile_definitions(nros_rmw_cyclonedds PRIVATE NROS_PLATFORM_FREERTOS)
    if(TARGET freertos_kernel)
        target_link_libraries(nros_rmw_cyclonedds PRIVATE freertos_kernel)
    endif()
    if(TARGET lwip)
        target_link_libraries(nros_rmw_cyclonedds PRIVATE lwip)
    endif()
# `MATCHES "^threadx"` (not STREQUAL): the invocations pass the variant names
# (`threadx_riscv64` / `threadx_linux` — the root CMakeLists maps them to the
# threadx platform module the same way). Pre-287-W6 the leaves baked the bare
# `set(NANO_ROS_PLATFORM threadx)`, which is why STREQUAL used to hold.
elseif(DEFINED NANO_ROS_PLATFORM AND NANO_ROS_PLATFORM MATCHES "^threadx")
    target_compile_definitions(nros_rmw_cyclonedds PRIVATE NROS_PLATFORM_THREADX)
    if(TARGET threadx_kernel)
        target_link_libraries(nros_rmw_cyclonedds PRIVATE threadx_kernel)
    endif()
    if(TARGET nros_platform_threadx)
        target_link_libraries(nros_rmw_cyclonedds PRIVATE nros_platform_threadx)
    endif()
endif()

# Propagate Cyclone's install lib dir as a build-tree rpath so test
# binaries find `libddsc.so` from `build/install/lib` instead of any
# system-installed `ros-humble-cyclonedds` that ld.so happens to see
# first. Only meaningful for the find_package path (an imported shared
# `libddsc.so`); a self-provisioned source build links a static `ddsc`
# in-tree, so there is no external `.so` to rpath. Phase 186.
set(_ddsc_loc "")
if(NROS_CYCLONEDDS_PROVENANCE STREQUAL "find_package")
    get_target_property(_ddsc_loc CycloneDDS::ddsc IMPORTED_LOCATION_RELEASE)
    if(NOT _ddsc_loc)
        get_target_property(_ddsc_loc CycloneDDS::ddsc IMPORTED_LOCATION)
    endif()
endif()
if(_ddsc_loc)
    get_filename_component(_ddsc_libdir "${_ddsc_loc}" DIRECTORY)
    set(NROS_RMW_CYCLONEDDS_DDSC_LIBRARY "${_ddsc_loc}"
        CACHE FILEPATH "Resolved Cyclone DDS ddsc library path")
    set_property(TARGET nros_rmw_cyclonedds APPEND PROPERTY
        INTERFACE_LINK_OPTIONS "LINKER:-rpath,${_ddsc_libdir}")
endif()

target_compile_options(nros_rmw_cyclonedds PRIVATE
    -Wall -Wextra -Wpedantic
    # Phase 171.E: keep the wrapper freestanding-friendly. Cyclone DDS
    # itself may allocate internally, but this wrapper must not rely on
    # exceptions, RTTI, or guard variables for local statics.
    -fno-exceptions -fno-rtti -fno-threadsafe-statics
    # Phase 204.13 — per-function/data sections so a `--gc-sections` link
    # (embedded + the gc-enabled native links) can drop the wrapper's unused
    # entity trampolines instead of pulling the whole archive member.
    -ffunction-sections -fdata-sections
)

set_target_properties(nros_rmw_cyclonedds PROPERTIES
    POSITION_INDEPENDENT_CODE ON
    EXPORT_NAME NrosRmwCyclonedds
)

# Phase 204.13 — opt-in interprocedural optimization (LTO) for size/perf-critical
# consumption. Off by default (cross-language link with `rust-lld` + slim-LTO
# ddsc objects is fragile — see 204.14); enable with
# `-DNROS_RMW_CYCLONEDDS_IPO=ON` where the toolchain supports it. Pair with
# `-DCMAKE_BUILD_TYPE=MinSizeRel` (`-Os`) for the smallest wrapper.
option(NROS_RMW_CYCLONEDDS_IPO "Enable IPO/LTO on the Cyclone RMW wrapper" OFF)
if(NROS_RMW_CYCLONEDDS_IPO)
    include(CheckIPOSupported)
    check_ipo_supported(RESULT _nros_ipo_ok OUTPUT _nros_ipo_msg)
    if(_nros_ipo_ok)
        set_target_properties(nros_rmw_cyclonedds PROPERTIES
            INTERPROCEDURAL_OPTIMIZATION ON)
    else()
        message(WARNING "NROS_RMW_CYCLONEDDS_IPO requested but unsupported: ${_nros_ipo_msg}")
    endif()
endif()

# ---- Tests -----------------------------------------------------------------
# Default the CTest harness ON only when this is the top-level project
# (`just cyclonedds build`). When add_subdirectory'd into an example or
# the root NanoRos build, the backend's own test fixtures must not be
# built — they generate descriptors for their own type set and are not
# part of the consuming app.
option(NROS_RMW_CYCLONEDDS_BUILD_TESTS "Build CTest harness" "${PROJECT_IS_TOP_LEVEL}")
if(NROS_RMW_CYCLONEDDS_BUILD_TESTS)
    enable_testing()
    add_subdirectory(tests)
endif()
