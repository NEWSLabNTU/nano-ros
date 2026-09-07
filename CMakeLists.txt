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
# phase-347 W3 — the FOURTH closed list of backends, and the one that had rotted
# furthest: it offered `dds`, retired in Phase 169, and omitted `uorb`, which the
# tree ships. Both the default and the cmake-gui choices come from the
# descriptors (`nros-rmw.toml`) rather than from a list kept here.
#
# phase-439 W4 — and they come from the provider SCAN now, not from a generated
# `NROS_RMW_KNOWN` literal that was written into `NanoRosRmwDispatch.cmake` when
# the `nros` binary was compiled. That literal could only ever name backends in
# THIS checkout; the scan reaches the user's workspace too (RFC-0094 D5).
# `SOFT` because these two lines are cosmetic — the help string and the
# cmake-gui drop-down — and a configure that cannot list the choices is still a
# configure. Selecting one is not cosmetic, and `nros_rmw_dispatch` below has no
# such arm.
include("${CMAKE_CURRENT_LIST_DIR}/cmake/NanoRosRmwDispatch.cmake")
nros_rmw_known(_nros_rmw_choices SOFT)
set(NANO_ROS_RMW "zenoh" CACHE STRING
    "RMW backend — one of: ${_nros_rmw_choices}")
set_property(CACHE NANO_ROS_RMW PROPERTY STRINGS ${_nros_rmw_choices})
set(NANO_ROS_PLATFORM "posix" CACHE STRING
    "Target platform (canonical: posix/freertos/nuttx/threadx; legacy: freertos_armcm3, nuttx_armv7a, threadx_linux, threadx_riscv64)")
set_property(CACHE NANO_ROS_PLATFORM PROPERTY STRINGS
    posix freertos nuttx threadx esp_idf
    freertos_armcm3 nuttx_armv7a threadx_linux threadx_riscv64)
# phase-405 W3 — the default and the valid list come from the one module that
# is allowed to spell an edition, so adding an edition is one edit.
include("${CMAKE_CURRENT_LIST_DIR}/cmake/NanoRosRosEdition.cmake")
string(REPLACE ";" ", " _nros_known_editions "${NANO_ROS_KNOWN_ROS_EDITIONS}")
set(NANO_ROS_ROS_EDITION "${NANO_ROS_DEFAULT_ROS_EDITION}" CACHE STRING
    "ROS 2 edition: ${_nros_known_editions}")
set_property(CACHE NANO_ROS_ROS_EDITION PROPERTY STRINGS ${NANO_ROS_KNOWN_ROS_EDITIONS})

# phase-261 W5 — declared capability axes, the C/C++ projection of `system.toml`
# `[system].features = [...]` (RFC-0004). A user sets this BEFORE pulling nano-ros
# in (`set(NANO_ROS_FEATURES "safety")`), or the bake emits it into
# `system_config.cmake`. `nros_lower_system_features` lowers each axis to its build
# knob (e.g. `safety` → `NANO_ROS_SAFETY_E2E ON`) HERE, before the core libs'
# `add_subdirectory` below evaluates the matching `option()` — so a declared axis
# reaches the C/C++ build, not just the informational `system_config.h` `#define`.
set(NANO_ROS_FEATURES "" CACHE STRING
    "Declared nano-ros capability axes (e.g. safety) — C/C++ projection of system.toml [system].features")
include("${CMAKE_CURRENT_SOURCE_DIR}/cmake/NanoRosCapabilities.cmake")
nros_lower_system_features("${NANO_ROS_FEATURES}")

# phase-314 — the ONE cargo feature-set computation, shared by nros-c, nros-cpp
# and the workspace umbrella. Included here so every consumer path sees it.
include("${CMAKE_CURRENT_SOURCE_DIR}/cmake/NanoRosFeatureSet.cmake")

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
# Issue 0390 — PREFER the provisioned copy; fetch only as a fallback.
#
# `[tool.corrosion]` in `nros-sdk-index.toml` pins 0.5.1-nros1 and
# `nros setup --tool corrosion` installs it into the SDK store, but this block
# used to FetchContent unconditionally — so the pinned tool was ignored, every
# cmake configure needed network, and the build resolved whatever
# `corrosion-rs/corrosion@v0.6.1` happened to be. On a host where the git
# credential helper is unhealthy the clone fails outright and the C fixture
# build dies four layers down in `corrosion-populate-download`, which is how
# this was found.
#
# issue 0493 — the store lookup, the fallback and the REPORT all moved into
# `cmake/NanoRosCorrosion.cmake`, because this block was one of three
# disagreeing answers to "which Corrosion?" (the other two were
# `nano_ros_workspace_metadata.cmake`'s bare `find_package` and
# `compile-check-fixtures.sh`'s inline `CMAKE_PREFIX_PATH`). Its glob assumed
# the versioned `corrosion/<ver>/` layout that `nros setup` writes and could not
# see the FLAT one `just workspace install-corrosion` writes, so on a host with
# an install present it still fell through to the network — which is how the two
# 0493 investigations measured two different target-dir topologies and both were
# right. `nros_resolve_corrosion()` handles both layouts and prints what it
# resolved.
include("${CMAKE_CURRENT_SOURCE_DIR}/cmake/NanoRosCorrosion.cmake")
nros_resolve_corrosion()

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
    # Phase 241.D3-rev — single-runtime model: the umbrella FFI staticlib
    # (`libnros_c.a` for C, `libnros_cpp.a` for C++) bundles the selected RMW backend
    # as an rlib dep, so each binary links exactly ONE Rust staticlib (one `std`).
    # No provider archive, no `external-registry` feature — the cffi `REGISTRY` + C
    # entry points are defined once in the single archive.
    add_subdirectory(packages/rmw/cffi)
    add_subdirectory(packages/api/nros-c)
    add_subdirectory(packages/api/nros-cpp)
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
    # `packages/api/nros-c/CMakeLists.txt` for Phase 137). Skipped for
    # NuttX cross builds where the FFI crate's cargo build supplies
    # nros-c via its own dependency closure.
    if(TARGET nros_c-static)
        # Phase 241.D3-rev — the C umbrella is the ONLY Rust staticlib on a C link
        # line ⇒ `std` once ⇒ no `--allow-multiple-definition`. Linked NORMALLY (not
        # whole-archive): whole-archiving a Rust staticlib force-includes its
        # `compiler-builtins` intrinsics (`__popcountdi2`, `__mulsc3`, …) which then
        # collide with the system `libgcc.a` — a dup the old flag used to mask. The C
        # surface is pulled lazily by the binary's own references, and the backend is
        # force-registered by the NanoRosLink.cmake `nros_app_register_backends()`
        # stub (it calls `nros_rmw_<x>_register`, defined in this umbrella).
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
            "${CMAKE_CURRENT_SOURCE_DIR}/packages/platform/nros-platform-api/include"
            "${CMAKE_CURRENT_SOURCE_DIR}/packages/api/nros-c/include"
            "${CMAKE_CURRENT_SOURCE_DIR}/packages/api/nros-cpp/include"
            "${CMAKE_CURRENT_SOURCE_DIR}/packages/core/nros-rmw-abi/include")
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
            # issue 0551 — through `nros_nuttx_include_root`, not
            # `${NUTTX_DIR}/include`. The shared tree holds whichever arch was
            # configured last, and no `nuttx/config.h` at all after a
            # `make olddefconfig`; the accessor prefers this arch's export
            # snapshot, which is what the comment above always meant.
            nros_nuttx_include_root(_nros_nuttx_inc)
            if(EXISTS "${_nros_nuttx_inc}/cxx")
                target_include_directories(NanoRos SYSTEM INTERFACE "${_nros_nuttx_inc}/cxx")
            endif()
            target_include_directories(NanoRos SYSTEM INTERFACE "${_nros_nuttx_inc}")
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
# RMW link dispatch — phase-439 W4 (RFC-0094 D5).
#
# WHAT THIS REPLACES
#
# An `if/elseif/else` chain over backend NAMES, closing on
#
#     else()
#         message(FATAL_ERROR "Unknown NANO_ROS_RMW: '${NANO_ROS_RMW}' "
#             "(expected: zenoh, xrce, or cyclonedds)")
#
# — the SECOND closed list of backends in the tree and the one that decides
# (issue 1215). Its contradiction sat 370 lines up in this same file: `:29`
# advertises `${NROS_RMW_KNOWN}` in the cmake-gui drop-down, which has named
# `uorb` since phase-347 W3, and this arm hard-failed on it. So `uorb` — an
# in-tree, zero-Rust C++ backend that announces itself, ships a descriptor and
# builds standalone in `just check rmw-uorb` — was advertised and unselectable,
# and the `NROS_RMW_CMAKE_TARGET` guard in `nros-cpp/CMakeLists.txt` written to
# link it had never once fired, because no arm ever created the target.
#
# WHAT DECIDES NOW
#
# The backend's own `[rmw.link]` declaration, read through `nros_rmw_dispatch`.
# The three arms were never three backends — they were three LINK STRATEGIES,
# and a backend knows which one it wants:
#
#   umbrella  the backend is a Rust rlib BUNDLED into `libnros_c.a` /
#             `libnros_cpp.a` and anchored there. Nothing separate reaches the
#             link line, so there is nothing to do here. (zenoh, xrce)
#   cmake     the backend is a CMake project. add_subdirectory its declared
#             dir and whole-archive its declared target into both umbrellas.
#             (cyclonedds, uorb, and any provider a user ships)
#
# Adding a backend is a `package.xml` announcement plus an `nros-rmw.toml`. It
# is NOT an edit here, which is RFC-0094's acceptance A2 and RFC-0071 D5's
# original one: "a fifth backend added out-of-tree with zero core edits."
# ---------------------------------------------------------------------------
nros_rmw_dispatch("${NANO_ROS_RMW}")

# ---------------------------------------------------------------------------
# _nros_link_cmake_rmw(<target>) — wire a CMake-provided backend into <target>.
#
# ONE implementation for the two consumers. This block used to exist twice,
# ~45 lines apart, differing only in the target name — and its own comments
# said so ("Issue 0837 — same flag, same missing edge; see the C umbrella
# above. One property, both consumers."). A second spelling of a link rule is
# how issue 0837 came to be a separate issue from 0475 in the first place.
#
# WHOLE-ARCHIVE, and why it is not `target_link_libraries`. The backend's
# registration object is referenced by nothing — it self-registers from a
# constructor — so a plain archive link drops it and the image boots with no
# backend and no diagnostic. And `target_link_libraries` with the archive
# BESIDE the flag is worse than useless: it reorders ld's single pass and
# breaks the whole-archive group outright (`undefined reference to ddsrt_*`,
# issue 0475). The archives ride ONE flag string; the rebuild edges come from
# `NANO_ROS_LINK_DEPEND_FILES` and `NanoRosLink.cmake`'s `LINK_DEPENDS`, which
# is the file edge a raw `-Wl,` flag cannot carry.
#
# `NROS_RMW_COMPANION_LIBRARIES` is what a backend APPENDS to that flag —
# cyclonedds names its `libddsc` there. Generic on purpose: the root has no
# business knowing that one backend has a companion library, and issue 0837
# was exactly the cost of a second archive in a flag with no channel to declare
# it through.
# ---------------------------------------------------------------------------
function(_nros_link_cmake_rmw _target)
    if(NOT TARGET ${_target})
        return()
    endif()
    # Raw `$<TARGET_FILE:...>` link flags carry no build-ORDER edge for
    # consumers of an INTERFACE library, so keep the dependency explicit.
    add_dependencies(${_target} ${NROS_RMW_CMAKE_TARGET})

    if(CMAKE_SYSTEM_NAME STREQUAL "Linux" OR CMAKE_SYSTEM_NAME MATCHES "BSD"
       OR CMAKE_SYSTEM_NAME STREQUAL "Generic")
        set(_flag "-Wl,--whole-archive,$<TARGET_FILE:${NROS_RMW_CMAKE_TARGET}>")
        foreach(_companion IN LISTS NROS_RMW_COMPANION_LIBRARIES)
            string(APPEND _flag ",${_companion}")
            # Issue 0837 — every FILE named inside that flag needs a rebuild
            # edge, not just the backend archive. `libddsc.a` rode the same
            # string with no edge, so a cyclonedds submodule bump rebuilt it and
            # relinked nothing: `lib/libddsc.a` at 18:06 against `c_talker` at
            # 15:45, with `cmake --build` exiting 0 having done nothing.
            # Recorded as a LIST on `NanoRos` so the link seam consumes whatever
            # the flag names — a third archive is covered by existing.
            set_property(TARGET NanoRos APPEND PROPERTY
                NANO_ROS_LINK_DEPEND_FILES "${_companion}")
            # phase-368 W4 — a companion named by ABSOLUTE PATH still records
            # only its SONAME in the link, so without an RPATH the runtime
            # loader searches the default paths and finds nothing on a host that
            # has no ROS. It worked on dev machines by ACCIDENT: a sourced ROS
            # env put ROS's own libddsc on LD_LIBRARY_PATH — a DIFFERENT build
            # of the library than the one linked. Stamp the linked library's own
            # directory on every consumer.
            get_filename_component(_companion_dir "${_companion}" DIRECTORY)
            if(_companion_dir)
                target_link_options(${_target} INTERFACE
                    "-Wl,-rpath,${_companion_dir}")
            endif()
        endforeach()
        string(APPEND _flag ",--no-whole-archive")
        target_link_libraries(${_target} INTERFACE "${_flag}")
    else()
        target_link_libraries(${_target} INTERFACE ${NROS_RMW_CMAKE_TARGET})
    endif()

    # A C++ backend in a C binary: the C driver does not pull in the C++
    # runtime, so `operator new`/`delete`/`std::nothrow` go unresolved.
    # Propagate through the INTERFACE; harmless for C++ apps. WHICH backends
    # need it is DECLARED (`[rmw.link] needs_cxx_linker`), not a name test.
    # issue 0835 — the property, not the label: see
    # `nros_platform_hosted_cxx_runtime` for why this used to read
    # `NANO_ROS_PLATFORM STREQUAL "threadx"` and why that was load-bearing on
    # six example leaves misreporting their platform.
    if(NROS_RMW_NEEDS_CXX_LINKER)
        nros_platform_hosted_cxx_runtime(_hosted_cxx "${NANO_ROS_PLATFORM}")
        if(_hosted_cxx)
            target_link_libraries(${_target} INTERFACE stdc++)
        endif()
    endif()
endfunction()

if(NROS_RMW_LINK_STRATEGY STREQUAL "umbrella")
    # Phase 241.D3-rev — the backend is BUNDLED into the umbrella staticlib
    # (`libnros_c.a` / `libnros_cpp.a`, built with the cffi feature the
    # descriptor names) and force-linked + auto-registered from there. The
    # standalone `nros-rmw-{zenoh,xrce}-cffi-staticlib` archives and the `-u`
    # register forcing are retired — there is no separate backend archive to
    # link, which is what `[rmw.link] rlib_dep` says.
elseif(NROS_RMW_LINK_STRATEGY STREQUAL "cmake")
    # A backend that needs something resolved BEFORE its own configure ships a
    # fragment beside its descriptor. The root includes it blind: what a
    # backend needs provisioned is the backend's business, and the alternative
    # is the `if(NANO_ROS_RMW STREQUAL "cyclonedds")` pre-step this wave
    # deleted. cyclonedds uses it to default `CYCLONEDDS_SOURCE_DIR` to the
    # pinned submodule so a bare cmake build needs no `just cyclonedds`.
    if(EXISTS "${NROS_RMW_CMAKE_DIR}/nros-rmw-provision.cmake")
        include("${NROS_RMW_CMAKE_DIR}/nros-rmw-provision.cmake")
    endif()
    if(NOT NROS_RMW_CMAKE_TARGET)
        message(FATAL_ERROR
            "nano-ros: rmw '${NROS_RMW_NAME}' declares the `cmake` link "
            "strategy but names no target. Add "
            "`[rmw.provides.cmake] target = \"<cmake target>\"` to "
            "${NROS_RMW_PACKAGE_DIR}/nros-rmw.toml.")
    endif()
    # A binary directory named after the backend, not after its source path:
    # an out-of-tree provider is outside `CMAKE_SOURCE_DIR`, and
    # `add_subdirectory()` REQUIRES an explicit binary dir for one.
    add_subdirectory("${NROS_RMW_CMAKE_DIR}"
        "${CMAKE_BINARY_DIR}/nros-rmw-${NROS_RMW_NAME}")
    if(TARGET ${NROS_RMW_CMAKE_TARGET})
        _nros_link_cmake_rmw(NanoRos)
        # Phase 241.D3-rev — the C++ umbrella no longer links the C one, so it
        # must pull the backend itself. One defect, two consumers, one helper.
        _nros_link_cmake_rmw(nros-cpp-headers)
    else()
        message(FATAL_ERROR
            "nano-ros: rmw '${NROS_RMW_NAME}' declares cmake target "
            "'${NROS_RMW_CMAKE_TARGET}', which its own CMakeLists.txt "
            "(${NROS_RMW_CMAKE_DIR}) did not create. The descriptor and the "
            "project disagree; nothing downstream can link this backend.")
    endif()
else()
    # Not a closed list of NAMES — a closed list of STRATEGIES, which is a
    # property of this file (it implements them) rather than of the tree.
    message(FATAL_ERROR
        "nano-ros: rmw '${NROS_RMW_NAME}' declares link strategy "
        "'${NROS_RMW_LINK_STRATEGY}', which this build does not implement "
        "(known: umbrella, cmake). See `[rmw.link] strategy` in "
        "${NROS_RMW_PACKAGE_DIR}/nros-rmw.toml.")
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
