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
        # Issue 0601 — a find_package MATCH is not a usable Cyclone.
        #
        # On a host with ROS installed, find_package resolves to
        # /opt/ros/humble, whose `idlc` links `libiceoryx_binding_c.so` from
        # ROS's own lib/. That library is on the loader path only when
        # `setup.bash` has been sourced INTO THE BUILD's environment — not
        # merely into the shell that launched cmake. So the config is found, the
        # target is imported, and the first `.idl` dies with
        #
        #     idlc: error while loading shared libraries: libiceoryx_binding_c.so
        #     FAILED: [code=127] …
        #
        # which reads as "command not found" for a tool that is right there.
        # Running `just cyclonedds setup` does not help: it provisions a working
        # Cyclone that this discovery never reaches.
        #
        # Selection by EXISTENCE where the property is RUNNABILITY — the same
        # shape as issue 0500's prefix ordering, and as the rosidl-adapter
        # ladder fixed alongside this. So ASK the tool. If it cannot run, this
        # install is not a candidate and we fall through to self-provisioning
        # from source, which is what a host without ROS already does.
        #
        # Deliberately narrow: only `idlc` is probed, because it is the only
        # part of the package this build EXECUTES. A library that fails to load
        # is caught by the linker with a legible error; a code generator that
        # fails to load is caught as `code=127` mid-ninja.
        if(CycloneDDS_FOUND AND TARGET CycloneDDS::idlc)
            get_target_property(_nros_idlc CycloneDDS::idlc IMPORTED_LOCATION)
            if(NOT _nros_idlc)
                # Config-specific installs (Release/None/…) put it here instead.
                get_target_property(_nros_idlc_cfgs CycloneDDS::idlc IMPORTED_CONFIGURATIONS)
                foreach(_cfg IN LISTS _nros_idlc_cfgs)
                    if(NOT _nros_idlc)
                        get_target_property(_nros_idlc CycloneDDS::idlc IMPORTED_LOCATION_${_cfg})
                    endif()
                endforeach()
            endif()
            if(_nros_idlc AND EXISTS "${_nros_idlc}")
                # `-h`, NOT `--version`: idlc has no `--version`, and a
                # WORKING one exits 1 on it ("invalid option"). Probing with
                # `--version` would therefore reject every healthy install and
                # silently force source-provisioning everywhere. Measured on
                # this host:
                #
                #   working idlc:  -h -> 0    --version -> 1
                #   broken  idlc:  -h -> 127  --version -> 127
                #
                # So `-h` separates "cannot load" from "loaded fine", and
                # `--version` separates nothing.
                execute_process(
                    COMMAND "${_nros_idlc}" -h
                    RESULT_VARIABLE _nros_idlc_rc
                    OUTPUT_QUIET ERROR_VARIABLE _nros_idlc_err)
                if(NOT _nros_idlc_rc EQUAL 0)
                    string(STRIP "${_nros_idlc_err}" _nros_idlc_err)
                    message(STATUS
                        "nano-ros: ignoring CycloneDDS at ${CycloneDDS_DIR} — its idlc "
                        "cannot run (${_nros_idlc}): ${_nros_idlc_err}")
                    message(STATUS
                        "nano-ros: falling back to a source-provisioned CycloneDDS (issue 0601)")
                    set(CycloneDDS_FOUND FALSE)
                endif()
            endif()
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
            # issue 0832 — ddsrt's heap goes through `nros_platform_alloc`, not
            # libc. Set on the ddsrt targets rather than globally: the switch is
            # read by ONE fork TU (src/ddsrt/src/heap/*/heap.c), and a global
            # define would also reach idlc and the confgen host tools, which
            # link no platform layer. `ddsrt` is the object library the static
            # `ddsc` absorbs; `ddsrt-internal` is its tools-side twin, which
            # must NOT get the define for exactly that reason.
            # `ddsrt` is an INTERFACE target in this layout — its sources
            # compile INSIDE `ddsc`, so that is where the define has to land.
            # `ddsrt-internal` (the tools-side twin that idlc/confgen link) is
            # deliberately left alone: those hosts link no platform layer.
            foreach(_nros_cdds_tgt ddsc ddsrt)
                if(TARGET ${_nros_cdds_tgt})
                    get_target_property(_nros_cdds_type ${_nros_cdds_tgt} TYPE)
                    if(NOT _nros_cdds_type STREQUAL "INTERFACE_LIBRARY")
                        target_compile_definitions(${_nros_cdds_tgt}
                            PRIVATE NROS_DDSRT_PLATFORM_FUNNEL)
                    endif()
                endif()
            endforeach()
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
