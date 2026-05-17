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
# No `just install-local` / `find_package(NanoRos CONFIG)` required. The
# legacy install path stays supported until Phase 140.
# ---------------------------------------------------------------------------

# ---------------------------------------------------------------------------
# Options / cache vars (set BEFORE add_subdirectory in the user project).
# ---------------------------------------------------------------------------
option(NANO_ROS_BUILD_CODEGEN "Build the nros-codegen tool" ON)
option(NANO_ROS_FORCE_INSTALL "Emit install(...) rules even when consumed as a subdirectory" OFF)
set(NANO_ROS_RMW "zenoh" CACHE STRING "RMW backend: zenoh, xrce, or dds")
set_property(CACHE NANO_ROS_RMW PROPERTY STRINGS zenoh xrce dds cyclonedds)
set(NANO_ROS_PLATFORM "posix" CACHE STRING
    "Target platform: posix, freertos_armcm3, nuttx_armv7a, threadx_linux, threadx_riscv64")
set_property(CACHE NANO_ROS_PLATFORM PROPERTY STRINGS
    posix freertos_armcm3 nuttx_armv7a threadx_linux threadx_riscv64)
set(NANO_ROS_ROS_EDITION "humble" CACHE STRING "ROS 2 edition: humble or iron")
set_property(CACHE NANO_ROS_ROS_EDITION PROPERTY STRINGS humble iron)

# ---------------------------------------------------------------------------
# CPack — binary distribution archives (only meaningful when nano-ros is
# the top-level build; harmless otherwise).
# ---------------------------------------------------------------------------
if(PROJECT_IS_TOP_LEVEL)
    set(CPACK_PACKAGE_NAME        "nros")
    set(CPACK_PACKAGE_VENDOR      "nano-ros contributors")
    set(CPACK_PACKAGE_DESCRIPTION_SUMMARY
        "nano-ros: lightweight ROS 2 client for embedded real-time systems")
    set(CPACK_PACKAGE_VERSION     "${PROJECT_VERSION}")
    set(CPACK_GENERATOR           "TGZ;ZIP")
    set(CPACK_INCLUDE_TOPLEVEL_DIRECTORY ON)

    string(TOLOWER "${CMAKE_SYSTEM_NAME}" _sys)
    set(CPACK_PACKAGE_FILE_NAME
        "nros-${PROJECT_VERSION}-${_sys}-${CMAKE_SYSTEM_PROCESSOR}-${NANO_ROS_RMW}-${NANO_ROS_PLATFORM}")

    include(CPack)
endif()

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
# Platform shim (canonical `nros_platform_*` ABI). POSIX has a dedicated
# C-port `libnros_platform_posix.a`; other platforms (Zephyr / FreeRTOS /
# NuttX / ThreadX) ship the shim through their own RTOS integration which
# Phase 138 will wire in. For Phase 137 only POSIX is hooked up.
# ---------------------------------------------------------------------------
if(NANO_ROS_PLATFORM STREQUAL "posix")
    add_subdirectory(packages/core/nros-platform-posix)
endif()

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
    elseif(NANO_ROS_PLATFORM STREQUAL "freertos_armcm3")
        target_compile_definitions(NanoRos INTERFACE NROS_PLATFORM_FREERTOS)
    elseif(NANO_ROS_PLATFORM STREQUAL "nuttx_armv7a")
        target_compile_definitions(NanoRos INTERFACE NROS_PLATFORM_NUTTX)
    elseif(NANO_ROS_PLATFORM MATCHES "^threadx")
        target_compile_definitions(NanoRos INTERFACE NROS_PLATFORM_THREADX)
    endif()
    # POSIX host system libraries — same set NanoRosCTargets.cmake adds.
    if(NANO_ROS_PLATFORM STREQUAL "posix")
        if(UNIX AND NOT APPLE)
            target_link_libraries(NanoRos INTERFACE pthread dl m)
        elseif(APPLE)
            target_link_libraries(NanoRos INTERFACE
                pthread dl m "-framework Security" "-framework CoreFoundation")
        endif()
        # Canonical `nros_platform_*` symbols come from the C-port
        # `libnros_platform_posix.a` (post-Phase 123.A.1.x.2 the
        # Rust-side `nros-platform-posix` crate was deleted).
        if(TARGET nros_platform_posix)
            target_link_libraries(NanoRos INTERFACE nros_platform_posix)
        endif()
    endif()
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
    # Phase 137 gap — the XRCE backend has no add_subdirectory-friendly
    # CMake entry point. `packages/xrce/nros-rmw-xrce/` is a standalone
    # cmake project that expects `find_package(NrosRmwXrce)` against an
    # installed prefix (driven by `just xrce build-rmw`). Phase 138 / 139
    # will rip it into a wrapper module. For now, users selecting the
    # XRCE backend should keep using the legacy install-local + find_package
    # path. We surface a fatal error so misconfiguration is loud rather
    # than producing a half-linked binary.
    message(FATAL_ERROR
        "NANO_ROS_RMW=xrce is not yet supported via add_subdirectory(). "
        "Use the legacy install-local path: `just install-local` + "
        "`find_package(NanoRos CONFIG)`. Phase 138/139 will close this gap.")
elseif(NANO_ROS_RMW STREQUAL "cyclonedds")
    message(FATAL_ERROR
        "NANO_ROS_RMW=cyclonedds is not yet supported via add_subdirectory(). "
        "Use the legacy install-local path: `just install-local` + "
        "`find_package(NanoRos CONFIG)`. Phase 138/139 will close this gap.")
else()
    message(FATAL_ERROR
        "Unknown NANO_ROS_RMW: '${NANO_ROS_RMW}' "
        "(expected: zenoh, xrce, dds, or cyclonedds)")
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
# Phase 137.2 — codegen module reachable from in-tree consumers.
#
# The function lives in the codegen submodule. We `include()` it
# directly so consumers reaching this root via `add_subdirectory` get
# `nros_generate_interfaces()` + `nros_find_interfaces()` without an
# install step. The install rule inside the codegen sub-project still
# copies it to `<prefix>/lib/cmake/NanoRos/` for legacy
# `find_package(NanoRos CONFIG)` consumers.
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
    "${CMAKE_CURRENT_SOURCE_DIR}/packages/codegen/packages/nros-codegen-c/cmake/NanoRosGenerateInterfaces.cmake")
if(EXISTS "${_nros_codegen_module}"
   AND NANO_ROS_BUILD_CODEGEN
   AND NANO_ROS_PLATFORM STREQUAL "posix"
   AND TARGET nros-codegen)
    set(_NANO_ROS_CODEGEN_TOOL "$<TARGET_FILE:nros-codegen>"
        CACHE INTERNAL "Path to nros C codegen tool (in-tree Corrosion target)")
    set(_NANO_ROS_PREFIX "${CMAKE_CURRENT_SOURCE_DIR}")
    include("${_nros_codegen_module}")
endif()

# ---------------------------------------------------------------------------
# Install rules — only fire when nano-ros is the top-level build (or the
# user opted in via -DNANO_ROS_FORCE_INSTALL=ON). When nano-ros is
# add_subdirectory'd into a user project, the user owns install layout.
# Sub-projects' own install(...) calls remain defined but are inert until
# the consumer invokes `cmake --install` against the top-level build.
# ---------------------------------------------------------------------------
if(PROJECT_IS_TOP_LEVEL OR NANO_ROS_FORCE_INSTALL)
    if(NANO_ROS_PLATFORM STREQUAL "posix")
        # Bundled interfaces (optional — codegen works via ament index when absent)
        if(IS_DIRECTORY "${PROJECT_SOURCE_DIR}/packages/codegen/interfaces")
            install(DIRECTORY packages/codegen/interfaces/
                DESTINATION share/nano-ros/interfaces
            )
        else()
            install(DIRECTORY DESTINATION share/nano-ros/interfaces)
        endif()
    endif()
endif()
