# Phase 186 — provide the `CycloneDDS::ddsc` target, resolving it in priority
# order so a bare `cmake`/`cargo` build is self-contained (no `just`/shell
# pre-step) while a user can still supply their own Cyclone.
#
# Order (every step user-overridable):
#   1. `CycloneDDS::ddsc` already defined  — a parent project provided it.
#   2. find_package(CycloneDDS CONFIG)     — a prebuilt install on
#      CMAKE_PREFIX_PATH / CycloneDDS_DIR (user install, or a `just`-built one).
#   3. self-provision from source          — add_subdirectory(${CYCLONEDDS_SOURCE_DIR});
#      defaults to the project's pinned third-party submodule (the root
#      CMakeLists sets it — the root legitimately owns third-party/), or the user
#      points it at their own Cyclone checkout.
#
# CMake-path convention (CLAUDE.md): this module never walks the source tree.
# The source location arrives as `CYCLONEDDS_SOURCE_DIR`; a standalone consumer
# with neither an install nor a source dir gets a clear, actionable error.
#
# Implemented as a MACRO (not a function): find_package's IMPORTED targets and
# add_subdirectory's targets must land in the caller's directory scope, which a
# function scope would not preserve. Sets `NROS_CYCLONEDDS_PROVENANCE` to one of
# target | find_package | source.

include_guard(GLOBAL)

# Per-platform Cyclone build knobs (WITH_FREERTOS/WITH_LWIP/WITH_THREADX, the
# BUILD_*/ENABLE_* feature trims, and the cross include flags) are staged by the
# caller as cache vars / CMAKE_C_FLAGS *before* invoking this macro — see
# cmake/platform/nano-ros-<plat>.cmake. The self-provision branch only wires
# sccache and the add_subdirectory.

macro(nros_provide_cyclonedds)
    if(TARGET CycloneDDS::ddsc)
        set(NROS_CYCLONEDDS_PROVENANCE "target")
        message(STATUS "nano-ros: CycloneDDS::ddsc already defined — reusing it")
    else()
        # On a cross build (CMAKE_TOOLCHAIN_FILE → CMAKE_CROSSCOMPILING), a
        # find_package(CycloneDDS) match is the HOST-native Cyclone (e.g. a
        # `~/.local` or ROS install): CycloneDDSConfig.cmake is arch-agnostic, so
        # find_package happily returns it, but its posix `ddsrt` headers
        # (`#include <sys/socket.h>`) do not exist on the freestanding embedded
        # target → the build dies compiling `iovec.h`. A prebuilt CROSS install
        # would have to be supplied as an already-defined `CycloneDDS::ddsc`
        # target (handled above); otherwise a cross build MUST self-provision
        # from source. So consult find_package only for native builds.
        if(CMAKE_CROSSCOMPILING)
            set(CycloneDDS_FOUND FALSE)
        else()
            find_package(CycloneDDS CONFIG QUIET)
        endif()
        if(CycloneDDS_FOUND)
            set(NROS_CYCLONEDDS_PROVENANCE "find_package")
            message(STATUS "nano-ros: CycloneDDS via find_package (${CycloneDDS_DIR})")
        elseif(CYCLONEDDS_SOURCE_DIR AND EXISTS "${CYCLONEDDS_SOURCE_DIR}/CMakeLists.txt")
            # sccache — route the Cyclone C/C++ compiles through sccache so the
            # objects become cache hits across example build trees instead of a
            # full per-example recompile (Phase 165.perf pattern). Degrades to a
            # direct compile when sccache is absent. Only set when the caller has
            # not already chosen a launcher.
            if(NOT DEFINED CMAKE_C_COMPILER_LAUNCHER)
                # `find_program` caches, and CMake never revalidates a cached
                # path. A build dir configured on the host and re-entered from
                # a container (the ROS distrobox — see
                # docs/development/ros2-on-non-ubuntu.md) then keeps the host's
                # `/usr/bin/sccache` as the compiler launcher, and every Cyclone
                # TU dies at `Error 127` — `/bin/sh: 1: /usr/bin/sccache: not
                # found` — which reads as a broken build, not a stale cache.
                # Drop the entry when it no longer resolves and search again.
                if(NROS_SCCACHE AND NOT EXISTS "${NROS_SCCACHE}")
                    message(STATUS
                        "nano-ros: cached sccache at ${NROS_SCCACHE} is gone "
                        "(different host or container) — re-detecting")
                    unset(NROS_SCCACHE CACHE)
                endif()
                find_program(NROS_SCCACHE sccache)
                if(NROS_SCCACHE)
                    set(CMAKE_C_COMPILER_LAUNCHER "${NROS_SCCACHE}")
                    set(CMAKE_CXX_COMPILER_LAUNCHER "${NROS_SCCACHE}")
                    message(STATUS "nano-ros: routing CycloneDDS build through sccache (${NROS_SCCACHE})")
                endif()
            endif()
            # ENABLE_LTO=OFF — issue 0492. Cyclone's own `option(ENABLE_LTO
            # "Enable link time optimization." ON)` makes GCC emit SLIM LTO
            # objects: `.gnu.lto_*` sections and a one-entry ELF symtab, with
            # the real symbols living in GCC IR. `nm` shows them (it loads
            # GCC's plugin) and `ld.bfd` links them (same plugin), but
            # **`ld.lld` cannot read GCC LTO IR at all** — and
            # `cmake/platform/nano-ros-posix.cmake` links with `-fuse-ld=lld`.
            #
            # The failure reads as a missing library and is not one:
            #
            #   ld.lld: error: undefined symbol: dds_get_guid
            #
            # while `libddsc.a` sits on the link line inside the whole-archive
            # group, `nm` reports `T dds_get_guid`, and `-Wl,-t` shows all 148
            # members loaded — including the one that defines it. Provisioning
            # a CycloneDDS (`nros setup --tool cyclonedds`) does not help
            # either, because this build sets
            # `CMAKE_DISABLE_FIND_PACKAGE_CycloneDDS=ON` and self-provisions.
            #
            # The RUST self-provision has always set this
            # (`cyclonedds-sys/build.rs`, "rust-lld cannot link slim-LTO
            # objects … same hazard on native"). The CMake self-provision —
            # the path every C/C++ example takes — did not. One of two sibling
            # paths fixed, which is the class this repo keeps re-paying for.
            #
            # Set BOTH spellings deliberately: the normal variable is what
            # `option()` honours under CMP0077 NEW, and the FORCEd cache entry
            # overwrites the `ENABLE_LTO:BOOL=ON` that existing build trees
            # already carry from an earlier configure — without it a stale tree
            # keeps building LTO objects and keeps failing.
            set(ENABLE_LTO OFF CACHE BOOL
                "nano-ros: ld.lld cannot link slim GCC-LTO objects (issue 0492)" FORCE)
            set(ENABLE_LTO OFF)
            message(STATUS "nano-ros: self-provisioning CycloneDDS from source: ${CYCLONEDDS_SOURCE_DIR}")
            # EXCLUDE_FROM_ALL: built only because nros_rmw_cyclonedds links
            # CycloneDDS::ddsc, not as part of `all`.
            add_subdirectory("${CYCLONEDDS_SOURCE_DIR}" "${CMAKE_CURRENT_BINARY_DIR}/_cyclonedds" EXCLUDE_FROM_ALL)
            # Where Cyclone generated its headers (dds/config.h, version.h, …) —
            # the backend needs this on the source path (see CMakeLists.txt).
            set(NROS_CYCLONEDDS_SOURCE_BUILD_DIR "${CMAKE_CURRENT_BINARY_DIR}/_cyclonedds")
            set(NROS_CYCLONEDDS_PROVENANCE "source")
        else()
            message(FATAL_ERROR
                "nano-ros: CycloneDDS not found and no source to build it from.\n"
                "  Supply ONE of:\n"
                "    -DCMAKE_PREFIX_PATH=<cyclonedds-install>          (use a prebuilt install)\n"
                "    -DCycloneDDS_DIR=<dir with CycloneDDSConfig.cmake>\n"
                "    -DCYCLONEDDS_SOURCE_DIR=<cyclonedds source tree>  (build from source)\n"
                "  The nano-ros project root defaults CYCLONEDDS_SOURCE_DIR to its pinned\n"
                "  third-party/dds/cyclonedds submodule; a standalone consumer must pass one.")
        endif()
    endif()
endmacro()
