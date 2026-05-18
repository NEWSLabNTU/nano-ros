cmake_minimum_required(VERSION 3.22)
project(NanoRos VERSION 0.1.0 LANGUAGES C CXX)

# ---------------------------------------------------------------------------
# Phase 137 — Top-level entry point for source-distribution consumption.
#
# Consumers can pull nano-ros into a project with:
#
#     set(NANO_ROS_PLATFORM posix)
#     set(NANO_ROS_RMW     zenoh)
#     add_subdirectory(third_party/nano-ros nano_ros)
#     target_link_libraries(my_app PRIVATE NanoRos::NanoRos)
#
# Phase 140 deleted `just install-local` + `find_package(NanoRos
# CONFIG)`; `add_subdirectory` is the only supported shape.
# ---------------------------------------------------------------------------

# ---------------------------------------------------------------------------
# Options / cache vars (set BEFORE add_subdirectory in the user project).
# ---------------------------------------------------------------------------
option(NANO_ROS_BUILD_CODEGEN "Build the nros-codegen tool" ON)
set(NANO_ROS_RMW "zenoh" CACHE STRING "RMW backend: zenoh, xrce, or dds")
set_property(CACHE NANO_ROS_RMW PROPERTY STRINGS zenoh xrce dds cyclonedds none)
set(NANO_ROS_PLATFORM "posix" CACHE STRING
    "Target platform (canonical: posix/freertos/nuttx/threadx; legacy: freertos_armcm3, nuttx_armv7a, threadx_linux, threadx_riscv64)")
set_property(CACHE NANO_ROS_PLATFORM PROPERTY STRINGS
    posix freertos nuttx threadx
    freertos_armcm3 nuttx_armv7a threadx_linux threadx_riscv64)
set(NANO_ROS_ROS_EDITION "humble" CACHE STRING "ROS 2 edition: humble or iron")
set_property(CACHE NANO_ROS_ROS_EDITION PROPERTY STRINGS humble iron)

# Phase 140 — CPack block removed alongside the install-local layout
# (CPack only ever bundled the install tree). Source-distribution
# consumers vendor the repo + run `add_subdirectory(nano-ros)` from
# their own project; no binary tarballs to ship.

# ---------------------------------------------------------------------------
# Phase 123.A.5 — auto-run tools/setup.sh if the user invoked cmake
# without sourcing setup first. Opt out via -DNANO_ROS_SKIP_BOOTSTRAP=ON.
# ---------------------------------------------------------------------------
include("${CMAKE_CURRENT_SOURCE_DIR}/cmake/bootstrap.cmake")

# ---------------------------------------------------------------------------
# Corrosion — integrates Rust/Cargo into CMake
# ---------------------------------------------------------------------------
include(FetchContent)
FetchContent_Declare(Corrosion
    GIT_REPOSITORY https://github.com/corrosion-rs/corrosion.git
    GIT_TAG        v0.6.1
)
FetchContent_MakeAvailable(Corrosion)

# ---------------------------------------------------------------------------
# Core libraries — always built.
# ---------------------------------------------------------------------------
add_subdirectory(packages/core/nros-rmw-cffi)
add_subdirectory(packages/core/nros-c)
add_subdirectory(packages/core/nros-cpp)

# ---------------------------------------------------------------------------
# Platform shim (canonical `nros_platform_*` ABI). Phase 138 dispatches
# via per-platform modules under `cmake/platform/`. Each module:
#   - Builds / surfaces the platform staticlib(s) for its target
#   - Exposes `NanoRos::Platform` ALIAS (linked into the umbrella below)
#   - Defines `nros_platform_link_app(target)` (per-app fixups —
#     link script, startup files, ISR vectors, RTOS-specific tweaks)
#   - Sets `NROS_PLATFORM_LINK_FEATURES` (default link feature set)
# Validation: the configured value must match a module on disk.
# ---------------------------------------------------------------------------
# Map legacy / variant NANO_ROS_PLATFORM values to their canonical
# per-platform module names. The legacy cache strings (e.g.
# `freertos_armcm3`, `threadx_riscv64`) name a specific platform+board
# pairing — Phase 138 collapses them to the platform axis (board moves
# to NANO_ROS_BOARD). Map here so existing recipes keep working.
set(_nros_platform_module_name "${NANO_ROS_PLATFORM}")
if(NANO_ROS_PLATFORM STREQUAL "freertos_armcm3")
    set(_nros_platform_module_name "freertos")
elseif(NANO_ROS_PLATFORM STREQUAL "nuttx_armv7a")
    set(_nros_platform_module_name "nuttx")
elseif(NANO_ROS_PLATFORM MATCHES "^threadx")
    set(_nros_platform_module_name "threadx")
endif()
set(_nros_platform_module
    "${CMAKE_CURRENT_SOURCE_DIR}/cmake/platform/nano-ros-${_nros_platform_module_name}.cmake")
if(NOT EXISTS "${_nros_platform_module}")
    message(FATAL_ERROR
        "Unknown NANO_ROS_PLATFORM: '${NANO_ROS_PLATFORM}' "
        "(no module at cmake/platform/nano-ros-${_nros_platform_module_name}.cmake). "
        "Supported: posix, zephyr, freertos, nuttx, threadx, baremetal "
        "(plus legacy freertos_armcm3 / nuttx_armv7a / threadx_linux / threadx_riscv64).")
endif()
include("${_nros_platform_module}")

# ---------------------------------------------------------------------------
# Phase 137.1 — in-tree NanoRos::NanoRos alias for source-vendored
# consumers. The `nros-c` sub-project builds `nros_c-static` (Corrosion
# staticlib target) but only declares `NanoRos::NanoRos` via the
# installed `NanoRosCTargets.cmake` (STATIC IMPORTED against an installed
# `libnros_c.a`). When pulled via `add_subdirectory(...)` no install
# happens, so we declare the umbrella INTERFACE target here and wire it
# to the in-tree Corrosion target + the RMW staticlib + (eventually)
# the platform shim.
# ---------------------------------------------------------------------------
if(NOT TARGET NanoRos)
    add_library(NanoRos INTERFACE)
    add_library(NanoRos::NanoRos ALIAS NanoRos)
    # `nros_c-static`'s INTERFACE carries the per-build variant header
    # path + the public `include/` dirs (wired in
    # `packages/core/nros-c/CMakeLists.txt` for Phase 137).
    target_link_libraries(NanoRos INTERFACE nros_c-static)
    # Propagate the platform compile definition to mirror the install-time
    # NanoRos::NanoRos behaviour from NanoRosCTargets.cmake.
    if(NANO_ROS_PLATFORM STREQUAL "posix")
        target_compile_definitions(NanoRos INTERFACE NROS_PLATFORM_POSIX)
    elseif(NANO_ROS_PLATFORM STREQUAL "freertos_armcm3"
           OR NANO_ROS_PLATFORM STREQUAL "freertos")
        target_compile_definitions(NanoRos INTERFACE NROS_PLATFORM_FREERTOS)
    elseif(NANO_ROS_PLATFORM STREQUAL "nuttx_armv7a"
           OR NANO_ROS_PLATFORM STREQUAL "nuttx")
        target_compile_definitions(NanoRos INTERFACE NROS_PLATFORM_NUTTX)
    elseif(NANO_ROS_PLATFORM MATCHES "^threadx")
        target_compile_definitions(NanoRos INTERFACE NROS_PLATFORM_THREADX)
    endif()
    # Platform shim wiring deferred to after RMW dispatch below —
    # `libnros_platform_<plat>.a` must appear AFTER `--whole-archive
    # libnros_rmw_<rmw>_staticlib.a --no-whole-archive` on the ld command
    # line so the platform symbols resolve back-references from the
    # whole-archive'd RMW objects. See ld(1) single-pass semantics.
endif()

# ---------------------------------------------------------------------------
# Phase 137.3 — RMW dispatch (inline form). Each branch pulls in the
# corresponding staticlib (or notes a gap) and links the RMW target into
# `NanoRos::NanoRos`. We keep the dispatch inline for Phase 137; per-RMW
# `cmake/rmw/nano-ros-rmw-<rmw>.cmake` modules land in Phase 138 once
# the per-platform layout (138 / 139) is in place.
# ---------------------------------------------------------------------------
if(NANO_ROS_RMW STREQUAL "zenoh")
    add_subdirectory(packages/zpico/nros-rmw-zenoh-staticlib)
    # `nros_rmw_zenoh_staticlib-static` is the Corrosion-emitted target
    # from the staticlib sub-project. Whole-archive wrap so the
    # auto-register .init_array ctor (phase 104.A) survives dead-strip.
    if(CMAKE_SYSTEM_NAME STREQUAL "Linux" OR CMAKE_SYSTEM_NAME MATCHES "BSD"
       OR CMAKE_SYSTEM_NAME STREQUAL "Generic")
        target_link_libraries(NanoRos INTERFACE
            "-Wl,--whole-archive"
            nros_rmw_zenoh_staticlib-static
            "-Wl,--no-whole-archive"
            "-Wl,--allow-multiple-definition")
    elseif(APPLE)
        target_link_libraries(NanoRos INTERFACE
            "-Wl,-force_load,$<TARGET_FILE:nros_rmw_zenoh_staticlib-static>"
            "-Wl,--allow-multiple-definition")
    else()
        target_link_libraries(NanoRos INTERFACE nros_rmw_zenoh_staticlib-static)
    endif()
elseif(NANO_ROS_RMW STREQUAL "dds")
    # Phase 137.1 — dust-DDS via the standalone staticlib. Build it via
    # Corrosion in-tree (parallels the zenoh-staticlib pattern). The
    # crate lives at packages/dds/nros-rmw-dds-staticlib/.
    corrosion_import_crate(
        MANIFEST_PATH "${CMAKE_CURRENT_SOURCE_DIR}/Cargo.toml"
        CRATES        nros-rmw-dds-staticlib
        CRATE_TYPES   staticlib
        NO_DEFAULT_FEATURES
        FEATURES      platform-${NANO_ROS_PLATFORM} ros-${NANO_ROS_ROS_EDITION}
        LOCKED
    )
    if(CMAKE_SYSTEM_NAME STREQUAL "Linux" OR CMAKE_SYSTEM_NAME MATCHES "BSD"
       OR CMAKE_SYSTEM_NAME STREQUAL "Generic")
        target_link_libraries(NanoRos INTERFACE
            "-Wl,--whole-archive"
            nros_rmw_dds_staticlib-static
            "-Wl,--no-whole-archive"
            "-Wl,--allow-multiple-definition")
    elseif(APPLE)
        target_link_libraries(NanoRos INTERFACE
            "-Wl,-force_load,$<TARGET_FILE:nros_rmw_dds_staticlib-static>"
            "-Wl,--allow-multiple-definition")
    else()
        target_link_libraries(NanoRos INTERFACE nros_rmw_dds_staticlib-static)
    endif()
elseif(NANO_ROS_RMW STREQUAL "xrce")
    # Phase 144.3 — XRCE via `nros-rmw-xrce-cffi` (the Rust C-FFI shim
    # that wraps the header-only Micro XRCE-DDS client + auto-registers
    # the RMW vtable via `linkme`). The cffi crate has no
    # `platform-*` / `ros-*` features (Phase 129.C.1 deleted them in
    # favour of `target_os`-driven selection); we pass `linkme-register`
    # (default) and `std` on hosted targets so Cargo can resolve the
    # panic handler when emitting the staticlib top-level. RTOS
    # consumers keep the default `no_std` build and supply their own
    # panic handler at the top-level link.
    set(_xrce_features linkme-register)
    if(NANO_ROS_PLATFORM STREQUAL "posix")
        list(APPEND _xrce_features std)
    endif()
    corrosion_import_crate(
        MANIFEST_PATH "${CMAKE_CURRENT_SOURCE_DIR}/Cargo.toml"
        CRATES        nros-rmw-xrce-cffi
        CRATE_TYPES   staticlib
        NO_DEFAULT_FEATURES
        FEATURES      ${_xrce_features}
        LOCKED
    )
    if(CMAKE_SYSTEM_NAME STREQUAL "Linux" OR CMAKE_SYSTEM_NAME MATCHES "BSD"
       OR CMAKE_SYSTEM_NAME STREQUAL "Generic")
        target_link_libraries(NanoRos INTERFACE
            "-Wl,--whole-archive"
            nros_rmw_xrce_cffi-static
            "-Wl,--no-whole-archive"
            "-Wl,--allow-multiple-definition")
    elseif(APPLE)
        target_link_libraries(NanoRos INTERFACE
            "-Wl,-force_load,$<TARGET_FILE:nros_rmw_xrce_cffi-static>"
            "-Wl,--allow-multiple-definition")
    else()
        target_link_libraries(NanoRos INTERFACE nros_rmw_xrce_cffi-static)
    endif()
elseif(NANO_ROS_RMW STREQUAL "cyclonedds")
    # Phase 144.fix — Cyclone DDS is a standalone C++ CMake project
    # (NOT a Rust crate via corrosion). It builds a libnros_rmw_cyclonedds.a
    # that registers the C++ vtable via nros_rmw_cffi_register at .init_array
    # time. add_subdirectory invokes the project's own CMakeLists.txt;
    # CMAKE_PREFIX_PATH must already point at the Cyclone DDS install —
    # users run `just cyclonedds setup` first to populate Cyclone DDS
    # itself (third-party C++ library, not part of nano-ros).
    add_subdirectory(packages/dds/nros-rmw-cyclonedds)
    if(TARGET nros_rmw_cyclonedds)
        if(CMAKE_SYSTEM_NAME STREQUAL "Linux" OR CMAKE_SYSTEM_NAME MATCHES "BSD"
           OR CMAKE_SYSTEM_NAME STREQUAL "Generic")
            target_link_libraries(NanoRos INTERFACE
                "-Wl,--whole-archive"
                nros_rmw_cyclonedds
                "-Wl,--no-whole-archive"
                "-Wl,--allow-multiple-definition")
        elseif(APPLE)
            target_link_libraries(NanoRos INTERFACE
                "-Wl,-force_load,$<TARGET_FILE:nros_rmw_cyclonedds>"
                "-Wl,--allow-multiple-definition")
        else()
            target_link_libraries(NanoRos INTERFACE nros_rmw_cyclonedds)
        endif()
    endif()
elseif(NANO_ROS_RMW STREQUAL "none")
    # Phase 156 — bridge / multi-backend consumers pull their own
    # RMW staticlibs via `corrosion_import_crate` to ensure
    # IDENTICAL feature sets across every `nros-rmw-cffi`
    # invocation (linkme distributed-slice dedup). Root CMake
    # skips the auto-pull when `NANO_ROS_RMW=none`. Consumers
    # MUST call `nros_rmw_<backend>::register()` (Rust) or
    # `nros_rmw_<backend>_register()` (C) explicitly from
    # their `main` / `nros_app_register_backends` — auto-
    # registration via linkme is disabled in this mode.
else()
    message(FATAL_ERROR
        "Unknown NANO_ROS_RMW: '${NANO_ROS_RMW}' "
        "(expected: zenoh, xrce, dds, cyclonedds, or none)")
endif()

# ---------------------------------------------------------------------------
# Platform shim link — appended AFTER the RMW staticlib so the
# `--whole-archive` RMW objects' undefined `nros_platform_*` refs
# resolve against `libnros_platform_<plat>.a` later on the ld command
# line. Per-platform module created `NanoRos::Platform` (ALIAS over
# `nros_platform_<plat>_iface`) when `cmake/platform/nano-ros-<plat>.cmake`
# was included earlier.
# ---------------------------------------------------------------------------
if(TARGET NanoRos AND TARGET NanoRos::Platform)
    target_link_libraries(NanoRos INTERFACE NanoRos::Platform)
endif()

# ---------------------------------------------------------------------------
# Codegen tool and bundled interfaces are host-only artifacts; skip them
# when cross-compiling for an embedded target.
# ---------------------------------------------------------------------------
if(NANO_ROS_PLATFORM STREQUAL "posix")
    if(NANO_ROS_BUILD_CODEGEN AND IS_DIRECTORY "${CMAKE_CURRENT_SOURCE_DIR}/packages/codegen/packages/nros-codegen-c")
        add_subdirectory(packages/codegen/packages/nros-codegen-c)
    endif()
endif()

# ---------------------------------------------------------------------------
# Phase 137.2 / 140 — codegen module reachable from in-tree consumers.
#
# The function lives in the codegen submodule. We `include()` it
# directly so consumers reaching this root via `add_subdirectory` get
# `nros_generate_interfaces()` + `nros_find_interfaces()` without an
# install step (Phase 140 — no install layout exists any more).
#
# The codegen module runs `find_program(nros-codegen ...)` at include
# time. In-tree consumers don't have an installed `<prefix>/bin/nros-codegen`,
# so we point `_NANO_ROS_CODEGEN_TOOL` at the Corrosion-emitted binary
# location BEFORE include — the module's eager find_program block
# short-circuits when the cache var is already set. The binary itself
# is produced once the user runs `cmake --build`; that is fine because
# the codegen module only invokes the tool at build time (custom
# commands), not at configure time.
# ---------------------------------------------------------------------------
set(_nros_codegen_module
    "${CMAKE_CURRENT_SOURCE_DIR}/cmake/NanoRosGenerateInterfaces.cmake")
if(EXISTS "${_nros_codegen_module}"
   AND NANO_ROS_BUILD_CODEGEN
   AND NANO_ROS_PLATFORM STREQUAL "posix"
   AND TARGET nros-codegen)
    set(_NANO_ROS_CODEGEN_TOOL "$<TARGET_FILE:nros-codegen>"
        CACHE INTERNAL "Path to nros C codegen tool (in-tree Corrosion target)")
    set(_NANO_ROS_PREFIX "${CMAKE_CURRENT_SOURCE_DIR}")
    include("${_nros_codegen_module}")
endif()

# Phase 140 — install-mode branch deleted. nano-ros is consumed
# exclusively as a source distribution via `add_subdirectory(<repo>)`
# (or the Phase 139 integration shells under `integrations/`).
# No install(...) rules anywhere in-tree.
