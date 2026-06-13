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
set(NANO_ROS_RMW "zenoh" CACHE STRING "RMW backend: zenoh, xrce, or cyclonedds")
set_property(CACHE NANO_ROS_RMW PROPERTY STRINGS zenoh xrce dds cyclonedds)
set(NANO_ROS_PLATFORM "posix" CACHE STRING
    "Target platform (canonical: posix/freertos/nuttx/threadx; legacy: freertos_armcm3, nuttx_armv7a, threadx_linux, threadx_riscv64)")
set_property(CACHE NANO_ROS_PLATFORM PROPERTY STRINGS
    posix freertos nuttx threadx esp_idf
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
# Core libraries — always built EXCEPT on tier-3 cross targets (NuttX).
# nros-c / nros-cpp / nros-rmw-cffi Corrosion targets fall over on
# `armv7a-nuttx-eabihf` because they need `-Z build-std` to compile core
# from source. NuttX builds funnel through `nros_board_link_app()` →
# `cargo build` on `nros-nuttx-ffi`, which depends on these crates and
# carries its own `.cargo/config.toml` with `build-std`. The cmake
# Corrosion target is redundant + impossible there.
# ---------------------------------------------------------------------------
# 192.3: the nano-ros root dir (this file's dir), so subdirs needn't walk up
# `../../..` to reach it (e.g. nros-cpp's deferred NanoRos::NanoRos link).
set(NANO_ROS_ROOT_DIR "${CMAKE_CURRENT_SOURCE_DIR}")
if(NOT NANO_ROS_PLATFORM MATCHES "^nuttx")
    add_subdirectory(packages/core/nros-rmw-cffi)
    add_subdirectory(packages/core/nros-c)
    add_subdirectory(packages/core/nros-cpp)

    # RFC-0042 D3 / phase-241.D slice 4 — the single `nros-rmw-cffi` provider
    # archive. `nros-rmw-cffi` is now def-less (its `REGISTRY` + C ABI entry
    # points are Rust-mangled), so `libnros_c.a`/`libnros_cpp.a`/the RMW staticlib
    # no longer emit duplicate strong defs; this archive emits them exactly once
    # via `nros_rmw_cffi_export!{}`. Linked LAST (after the whole-archive RMW + the
    # platform shim) so its back-references resolve under single-pass ld — the same
    # trailing-archive discipline `NanoRos::Platform` uses. This is what lets the
    # RMW link drop `--allow-multiple-definition` (the blind ODR mask).
    set(_cffi_provider_features "")
    if(NANO_ROS_PLATFORM STREQUAL "posix")
        set(_cffi_provider_features platform-posix)
    elseif(NANO_ROS_PLATFORM MATCHES "freertos" OR NANO_ROS_PLATFORM STREQUAL "esp_idf")
        set(_cffi_provider_features platform-freertos)
    elseif(NANO_ROS_PLATFORM MATCHES "^threadx")
        set(_cffi_provider_features platform-threadx)
    endif()
    corrosion_import_crate(
        MANIFEST_PATH "${CMAKE_CURRENT_SOURCE_DIR}/Cargo.toml"
        CRATES        nros-rmw-cffi-provider
        CRATE_TYPES   staticlib
        NO_DEFAULT_FEATURES
        FEATURES      ${_cffi_provider_features}
        LOCKED
    )
endif()

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
        "Supported: posix, zephyr, freertos, nuttx, threadx, baremetal, esp_idf "
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
    # `packages/core/nros-c/CMakeLists.txt` for Phase 137). Skipped for
    # NuttX cross builds where the FFI crate's cargo build supplies
    # nros-c via its own dependency closure.
    if(TARGET nros_c-static)
        target_link_libraries(NanoRos INTERFACE nros_c-static)
    else()
        # NuttX (and any future tier-3 cross build) — staticlibs come
        # in via the FFI crate's cargo build, but downstream cmake
        # targets (`nros_generate_interfaces()` output, user code) still
        # `#include <nros/...>` headers. Expose the public include
        # surfaces directly.
        target_include_directories(NanoRos INTERFACE
            # phase-243 — the one canonical `<nros/platform.h>` lives here now
            # (nros-c's legacy platform.h is deleted); list it first.
            "${CMAKE_CURRENT_SOURCE_DIR}/packages/core/nros-platform-api/include"
            "${CMAKE_CURRENT_SOURCE_DIR}/packages/core/nros-c/include"
            "${CMAKE_CURRENT_SOURCE_DIR}/packages/core/nros-cpp/include"
            "${CMAKE_CURRENT_SOURCE_DIR}/packages/core/nros-rmw-cffi/include")
    endif()
    # Phase 159 — NanoRos::NanoRosCpp ALIAS for NuttX path (where
    # nros-cpp's add_subdirectory is skipped). C++ examples
    # `target_link_libraries(... NanoRos::NanoRosCpp)`; without this
    # alias the cmake generate step errors with "links to target
    # NanoRos::NanoRosCpp but the target was not found". Make it
    # an alias of NanoRos so consumers get the same INTERFACE
    # includes (nros-cpp/include is already added above).
    if(NOT TARGET NanoRos::NanoRosCpp)
        add_library(NanoRos::NanoRosCpp ALIAS NanoRos)
    endif()
    # Propagate the platform compile definition to mirror the install-time
    # NanoRos::NanoRos behaviour from NanoRosCTargets.cmake.
    if(NANO_ROS_PLATFORM STREQUAL "posix")
        target_compile_definitions(NanoRos INTERFACE NROS_PLATFORM_POSIX)
    elseif(NANO_ROS_PLATFORM STREQUAL "freertos_armcm3"
           OR NANO_ROS_PLATFORM STREQUAL "freertos"
           OR NANO_ROS_PLATFORM STREQUAL "esp_idf")
        target_compile_definitions(NanoRos INTERFACE NROS_PLATFORM_FREERTOS)
    elseif(NANO_ROS_PLATFORM STREQUAL "nuttx_armv7a"
           OR NANO_ROS_PLATFORM STREQUAL "nuttx")
        target_compile_definitions(NanoRos INTERFACE NROS_PLATFORM_NUTTX)
        # issue-0027 #1 — NuttX reuses nros-c's posix platform header, which
        # calls clock_gettime/nanosleep. NuttX declares those unconditionally in
        # its own <time.h>, but the cross toolchain's bare newlib gates the POSIX
        # options behind `#ifdef __rtems__` (riscv-none-elf 14), so the cmake-built
        # message libs (which only see newlib) fail to compile. Put the NuttX
        # export include tree on the message-lib + example compile as a SYSTEM
        # include so <time.h> resolves to NuttX's. The FFI crate's cargo build
        # already adds this for the app TU; this covers the cmake C/C++ TUs.
        # NUTTX_DIR is validated by the platform module included above.
        if(NUTTX_DIR)
            # issue-0036 — C++ TUs: NuttX ships <cstdlib>/<cstdio>/… wrappers
            # under include/cxx that pull NuttX's own <stdlib.h>. Without this
            # dir ahead of the toolchain's libstdc++ headers, libstdc++ <cstdlib>
            # does `#include_next <stdlib.h>` and reaches the cross newlib
            # stdlib.h, whose div_t/ldiv_t/lldiv_t (anonymous-struct typedefs)
            # clash with NuttX's named-struct ones (`struct div_s`) arriving via
            # the direct <stdlib.h> ("conflicting declaration 'typedef struct
            # div_t div_t'"). Mirror the cargo FFI build
            # (nros-board-common::nuttx_ffi_build). C TUs ignore the
            # extension-less cxx wrappers, so this is safe for the shared
            # C/C++ interface. Listed first so it precedes the toolchain's
            # built-in C++ include dir in the -isystem search.
            if(EXISTS "${NUTTX_DIR}/include/cxx")
                target_include_directories(NanoRos SYSTEM INTERFACE "${NUTTX_DIR}/include/cxx")
            endif()
            target_include_directories(NanoRos SYSTEM INTERFACE "${NUTTX_DIR}/include")
        endif()
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
    # NuttX (and any future tier-3 target) — the FFI crate bundles
    # nros-rmw-zenoh via cargo with `-Z build-std`, so the Corrosion-
    # built `libnros_rmw_zenoh_staticlib.a` at the cmake level is both
    # redundant and impossible (Corrosion would invoke `cargo build`
    # against a tier-3 triple without the per-example `.cargo/config.toml`
    # that supplies build-std). Skip the staticlib add_subdirectory for
    # those platforms; `nros_board_link_app()` (board overlay) drives
    # the cargo build that produces the final ELF.
    if(NOT NANO_ROS_PLATFORM MATCHES "^nuttx")
        add_subdirectory(packages/zpico/nros-rmw-zenoh-staticlib)
        # `nros_rmw_zenoh_staticlib-static` is the Corrosion-emitted target
        # from the staticlib sub-project. Whole-archive wrap so the
        # auto-register .init_array ctor (phase 104.A) survives dead-strip.
        if(CMAKE_SYSTEM_NAME STREQUAL "Linux" OR CMAKE_SYSTEM_NAME MATCHES "BSD"
           OR CMAKE_SYSTEM_NAME STREQUAL "Generic")
            # RFC-0042 D3 — `--allow-multiple-definition` dropped (cffi is now
            # single-definition via the provider archive) AND `--whole-archive`
            # replaced by `-u` forcing of the backend register entry: whole-archive
            # also dragged the rmw staticlib's core/std COMDAT closure in, which
            # collided with the consumers' copies (the other half of what the flag
            # masked). Forcing only `nros_rmw_zenoh_register` lazy-links the rest.
            target_link_libraries(NanoRos INTERFACE
                "-Wl,-u,nros_rmw_zenoh_register"
                nros_rmw_zenoh_staticlib-static)
        elseif(APPLE)
            target_link_libraries(NanoRos INTERFACE
                "-Wl,-force_load,$<TARGET_FILE:nros_rmw_zenoh_staticlib-static>"
                "-Wl,--allow-multiple-definition")
        else()
            target_link_libraries(NanoRos INTERFACE nros_rmw_zenoh_staticlib-static)
        endif()
    endif()
elseif(NANO_ROS_RMW STREQUAL "xrce")
    # Phase 160.L — XRCE via `nros-rmw-xrce-cffi-staticlib` (the
    # standalone staticlib sibling that wraps the Rust C-FFI shim
    # `nros-rmw-xrce-cffi`). The wrapper exists because Corrosion's
    # `CRATE_TYPES staticlib` flag FILTERS — it does not FORCE the
    # crate-type emit. Putting `staticlib` directly on the cffi crate's
    # `[lib].crate-type` made Cargo resolve a `panic_handler` at
    # compile time on Zephyr / FreeRTOS no_std cross builds where the
    # example's panic source isn't visible (Phase 154's note). The
    # wrapper sibling carries the staticlib crate-type so the cffi
    # crate can stay rlib-only.
    set(_xrce_features linkme-register)
    if(NANO_ROS_PLATFORM STREQUAL "posix")
        list(APPEND _xrce_features std)
    endif()
    # Phase 212.M.7 — ESP-IDF integration links the staticlib into the
    # C `app_main.c` binary which carries no Rust-side panic_handler.
    # Opt the staticlib's `panic-halt` strategy in so compile-time
    # panic resolution succeeds. Other RTOS targets supply panic from
    # the top-level Rust binary and keep the staticlib lean.
    if(NANO_ROS_PLATFORM STREQUAL "esp_idf")
        list(APPEND _xrce_features panic-halt)
    endif()
    corrosion_import_crate(
        MANIFEST_PATH "${CMAKE_CURRENT_SOURCE_DIR}/Cargo.toml"
        CRATES        nros-rmw-xrce-cffi-staticlib
        CRATE_TYPES   staticlib
        NO_DEFAULT_FEATURES
        FEATURES      ${_xrce_features}
        LOCKED
    )
    if(CMAKE_SYSTEM_NAME STREQUAL "Linux" OR CMAKE_SYSTEM_NAME MATCHES "BSD"
       OR CMAKE_SYSTEM_NAME STREQUAL "Generic")
        # RFC-0042 D3 — same as the zenoh arm: cffi single-def via the provider +
        # `-u` register-force instead of whole-archive, so no flag is needed.
        target_link_libraries(NanoRos INTERFACE
            "-Wl,-u,nros_rmw_xrce_register"
            nros_rmw_xrce_cffi_staticlib-static)
    elseif(APPLE)
        # Untouched on macOS (no run_e2e cell to validate `-u`/force_load here).
        target_link_libraries(NanoRos INTERFACE
            "-Wl,-force_load,$<TARGET_FILE:nros_rmw_xrce_cffi_staticlib-static>"
            "-Wl,--allow-multiple-definition")
    else()
        target_link_libraries(NanoRos INTERFACE nros_rmw_xrce_cffi_staticlib-static)
    endif()
elseif(NANO_ROS_RMW STREQUAL "cyclonedds")
    # Phase 144.fix — Cyclone DDS is a standalone C++ CMake project
    # (NOT a Rust crate via corrosion). It builds a libnros_rmw_cyclonedds.a
    # that registers the C++ vtable via nros_rmw_cffi_register at .init_array
    # time. add_subdirectory invokes the project's own CMakeLists.txt.
    #
    # Phase 186 — the backend's nros_provide_cyclonedds() resolves Cyclone:
    # a prebuilt install on CMAKE_PREFIX_PATH wins; otherwise it self-provisions
    # from CYCLONEDDS_SOURCE_DIR. As the project root we own third-party/, so
    # default that to the pinned submodule — a bare cmake build then needs no
    # `just cyclonedds` pre-step. A user overrides with -DCMAKE_PREFIX_PATH
    # (their install) or -DCYCLONEDDS_SOURCE_DIR (their checkout).
    if(NOT DEFINED CYCLONEDDS_SOURCE_DIR
       AND EXISTS "${CMAKE_CURRENT_SOURCE_DIR}/third-party/dds/cyclonedds/CMakeLists.txt")
        set(CYCLONEDDS_SOURCE_DIR "${CMAKE_CURRENT_SOURCE_DIR}/third-party/dds/cyclonedds"
            CACHE PATH "CycloneDDS source tree for self-provisioning (Phase 186)")
    endif()
    add_subdirectory(packages/dds/nros-rmw-cyclonedds)
    if(TARGET nros_rmw_cyclonedds)
        # The Linux/BSD link path below whole-archives the backend via a raw
        # linker flag so the registration object cannot be dropped. Raw
        # `$<TARGET_FILE:...>` link flags do not add a build-order edge for
        # consumers of this INTERFACE library, so keep the dependency explicit.
        add_dependencies(NanoRos nros_rmw_cyclonedds)
        if(CMAKE_SYSTEM_NAME STREQUAL "Linux" OR CMAKE_SYSTEM_NAME MATCHES "BSD"
           OR CMAKE_SYSTEM_NAME STREQUAL "Generic")
            # RFC-0042 D3 — `--allow-multiple-definition` dropped (cffi single-def
            # via the provider). Whole-archive kept: Cyclone is C/C++ (libddsc + the
            # C++ wrapper), so it carries no Rust core/std COMDAT closure to collide
            # — the `-u` swap the Rust-staticlib backends need does not apply here.
            if(NROS_RMW_CYCLONEDDS_DDSC_LIBRARY)
                target_link_libraries(NanoRos INTERFACE
                    "-Wl,--whole-archive,$<TARGET_FILE:nros_rmw_cyclonedds>,${NROS_RMW_CYCLONEDDS_DDSC_LIBRARY},--no-whole-archive")
            else()
                target_link_libraries(NanoRos INTERFACE
                    "-Wl,--whole-archive,$<TARGET_FILE:nros_rmw_cyclonedds>,--no-whole-archive")
            endif()
        elseif(APPLE)
            target_link_libraries(NanoRos INTERFACE
                "-Wl,-force_load,$<TARGET_FILE:nros_rmw_cyclonedds>"
                "-Wl,--allow-multiple-definition")
        else()
            target_link_libraries(NanoRos INTERFACE nros_rmw_cyclonedds)
        endif()
        # Cyclone DDS's RMW wrapper is C++ (operator new/delete,
        # std::nothrow). A C application linking `NanoRos::NanoRos`
        # drives the link with the C compiler, which does NOT pull in
        # the C++ runtime — so C examples (e.g. examples/native/c/
        # cyclonedds/) fail with undefined `operator new`/`delete` /
        # `std::nothrow`. Propagate the C++ stdlib through the INTERFACE
        # so any C consumer resolves them. Harmless for C++ apps (the
        # C++ driver already links it).
        if(NOT APPLE
           AND NOT (DEFINED NANO_ROS_PLATFORM AND NANO_ROS_PLATFORM STREQUAL "threadx"))
            target_link_libraries(NanoRos INTERFACE stdc++)
        endif()
    endif()
else()
    message(FATAL_ERROR
        "Unknown NANO_ROS_RMW: '${NANO_ROS_RMW}' "
        "(expected: zenoh, xrce, or cyclonedds)")
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

# RFC-0042 D3 — link the cffi provider archive LAST (after the platform shim),
# lazy, so single-pass ld pulls just its needed objects (REGISTRY + the C entry
# points back-referenced by the whole-archive'd RMW + the consumers) without
# dragging its dependency closure (which the consumers already provide).
if(TARGET NanoRos AND TARGET nros_rmw_cffi_provider-static)
    target_link_libraries(NanoRos INTERFACE nros_rmw_cffi_provider-static)
endif()

# ---------------------------------------------------------------------------
# Phase 137.2 / 140 / 195.D — codegen module reachable from in-tree consumers.
#
# `nros_generate_interfaces()` + `nros_find_interfaces()` are provided by the
# in-tree `cmake/NanoRosGenerateInterfaces.cmake`; we `include()` it directly so
# consumers reaching this root via `add_subdirectory` get them without an
# install step (Phase 140 — no install layout exists any more).
#
# Phase 195.D retired the `packages/codegen` submodule; Phase 218 brought the
# CLI back in-tree as a sub-workspace at `packages/cli/`, built by
# `just setup-cli` (`source ./activate.sh` puts it on PATH). The module's
# `find_program(nros)` resolves it from PATH; a consumer may override with
# `-D_NANO_ROS_CODEGEN_TOOL=<path>`. The tool is only invoked at build time
# (custom commands), not at configure time.
# ---------------------------------------------------------------------------
set(_nros_codegen_module
    "${CMAKE_CURRENT_SOURCE_DIR}/cmake/NanoRosGenerateInterfaces.cmake")
if(EXISTS "${_nros_codegen_module}"
   AND NANO_ROS_PLATFORM STREQUAL "posix")
    set(_NANO_ROS_PREFIX "${CMAKE_CURRENT_SOURCE_DIR}")
    include("${_nros_codegen_module}")
endif()

# Phase 140 — install-mode branch deleted. nano-ros is consumed
# exclusively as a source distribution via `add_subdirectory(<repo>)`
# (or the Phase 139 integration shells under `integrations/`).
# No install(...) rules anywhere in-tree.
