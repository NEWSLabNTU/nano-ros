# nros Zephyr Module
# Copyright (c) 2024 nros contributors
# SPDX-License-Identifier: MIT OR Apache-2.0
#
# This module compiles backend-specific C sources centrally and bridges
# Kconfig values to Cargo environment variables, so individual examples
# don't need to manually include them.

if(CONFIG_NROS)

include(${CMAKE_CURRENT_LIST_DIR}/cmake/nros_cargo_build.cmake)
# Phase 212.H.1 — nros_system_generate(<bringup-pkg>) shells the
# `nros codegen-system` host-time bake at zephyr cmake configure
# time. Consumes <bringup>/system.toml + launch/*.xml; emits a baked
# tree under ${CMAKE_BINARY_DIR}/nros-system/ wired into `app`. See
# cmake/nros_system_generate.cmake (≤200 LoC adapter shim).
include(${CMAKE_CURRENT_LIST_DIR}/cmake/nros_system_generate.cmake)
# Phase 215.B.2 — nano_ros_use_board(<name>). Layers a board crate's
# board.cmake sidecar manifest (EXTRA_CONF_FILE / DTC_OVERLAY_FILE /
# BOARD / NANO_ROS_RMW / NROS_BOARD_RUNNER) into the downstream app
# build. The canonical call site is the consumer app's CMakeLists.txt
# BEFORE find_package(Zephyr); this include just makes the fn
# available wherever nano-ros is on ZEPHYR_EXTRA_MODULES (re-include
# is benign — cmake allows fn redefinition).
include(${CMAKE_CURRENT_LIST_DIR}/cmake/nano_ros_use_board.cmake)

# The nros repo root (parent of this zephyr/ directory).
# Phase 210.E.3.c — promoted to CACHE so consumer application's
# CMakeLists.txt can reference it (e.g.
# `include("${NROS_REPO_DIR}/cmake/compat/NrosRclcppCompat.cmake")`
# for the Phase 210 find_package shape). Without CACHE the var only
# lives in this directory's scope and the consumer sees an empty value.
set(NROS_REPO_DIR ${CMAKE_CURRENT_LIST_DIR}/.. CACHE PATH
    "nano-ros repo root (parent of zephyr/ module dir)")

# `zephyr_library()` would create a default library named after this
# directory's path (e.g. `..__..__nano-ros__zephyr`). All sources below
# are added via `zephyr_library_sources(...)` which targets the most
# recently declared library — the named one — so the unnamed one was
# always empty and Zephyr printed "No SOURCES given to Zephyr library:
# ..__..__nano-ros__zephyr". Use only the named form.
zephyr_library_named(nros)

# =============================================================================
# Platform shims (all RMW backends)
# =============================================================================

# Real-symbol wrappers around Zephyr kernel inlines (k_msleep, k_uptime_get,
# sys_rand_get, getaddrinfo) that nros-platform-zephyr needs to call via FFI.
zephyr_library_sources(${CMAKE_CURRENT_LIST_DIR}/nros_platform_zephyr_shims.c)
zephyr_include_directories(${NROS_REPO_DIR}/packages/core/nros-platform-api/include)
zephyr_library_sources(
    ${NROS_REPO_DIR}/packages/core/nros-platform-zephyr/src/platform.c
    ${NROS_REPO_DIR}/packages/core/nros-platform-zephyr/src/net.c
    ${NROS_REPO_DIR}/packages/core/nros-platform-zephyr/src/timer.c
    # Phase 200.1 — RMW-independent network-wait (compiled for every RMW; was
    # mis-filed as zpico_zephyr_wait_network in the zenoh-only zpico_zephyr.c).
    ${NROS_REPO_DIR}/packages/core/nros-platform-zephyr/src/net_wait.c
)

# phase-281 W3a — C/C++ multi-tier Zephyr entry seam
# (`nros_board_zephyr_run_tiers`, packages/boards/nros-board-zephyr/c/). A
# generated Zephyr C/C++ tiers entry emits `ZephyrBoard::run_tiers(...)` which
# calls this C seam; it opens ONE RMW session and spawns one `k_thread` per
# non-boot tier via the `nros_zephyr_tier_task_create` shim above (RFC-0015
# Model 1). It references nros_cpp_* (the typed runtime in nros-c / nros-cpp),
# so compile it only when the C or C++ API is present; it is inert (its symbol
# unreferenced) for single-tier / run_components entries. Added once here — the
# C++ API path always co-builds nros-c, so a single gate avoids adding the same
# source twice.
if(CONFIG_NROS_C_API OR CONFIG_NROS_CPP_API)
    zephyr_library_sources(
        ${NROS_REPO_DIR}/packages/boards/nros-board-zephyr/c/zephyr_run_tiers.c)
endif()

# =============================================================================
# Backend-specific C sources
# =============================================================================

set(NROS_ZEPHYR_DIR ${CMAKE_CURRENT_LIST_DIR})
include(${CMAKE_CURRENT_LIST_DIR}/cmake/nros_rmw_zenoh.cmake)
include(${CMAKE_CURRENT_LIST_DIR}/cmake/nros_rmw_xrce.cmake)
include(${CMAKE_CURRENT_LIST_DIR}/cmake/nros_rmw_cyclonedds.cmake)

if(CONFIG_NROS_RMW_ZENOH)
    nros_zephyr_configure_rmw_zenoh()
elseif(CONFIG_NROS_RMW_XRCE)
    nros_zephyr_configure_rmw_xrce()
elseif(CONFIG_NROS_RMW_CYCLONEDDS)
    nros_zephyr_configure_rmw_cyclonedds()
endif() # RMW backend

# =============================================================================
# Native linker workaround for Rust staticlibs (C and C++ API)
#
# Rust staticlibs built with std for x86_64-unknown-linux-gnu reference
# malloc/free, pulling picolibc's nano-malloc into the link. This conflicts
# with Zephyr's COMMON_LIBC_MALLOC. Fix:
#   --allow-multiple-definition: Zephyr's malloc wins (linked first)
#   heap_stub_native.c: provides __heap_start/__heap_end for picolibc's sbrk
# =============================================================================

if((CONFIG_NROS_C_API OR CONFIG_NROS_CPP_API) AND
   (CONFIG_BOARD_NATIVE_SIM OR CONFIG_BOARD_NATIVE_POSIX))
    zephyr_ld_options(-Wl,--allow-multiple-definition)
    zephyr_library_sources(${CMAKE_CURRENT_LIST_DIR}/heap_stub_native.c)
endif()

# C++ Zephyr builds can link several Rust staticlibs in one final ELF:
# nros-cpp, optional generated message FFI libs, and sometimes nros-c for
# shared C/C++ surfaces. Each no_std staticlib is a Rust binary artifact and
# therefore carries allocator, panic, and critical-section lang items. Let the
# first definition win at the final Zephyr link.
if(CONFIG_NROS_CPP_API AND
   NOT (CONFIG_BOARD_NATIVE_SIM OR CONFIG_BOARD_NATIVE_POSIX))
    zephyr_ld_options(-Wl,--allow-multiple-definition)
endif()

# =============================================================================
# C API path: build nros-c via Cargo and link it
# =============================================================================

if(CONFIG_NROS_C_API)
    # Phase 119.3: prepend `${CMAKE_BINARY_DIR}/nros-rust/nros-c-generated`
    # to include path. nros-c's build.rs writes the per-build header
    # there for this Zephyr build's exact storage sizes. The source-tree
    # header is a stub that `#error`s if reached.
    zephyr_include_directories(${CMAKE_BINARY_DIR}/nros-rust/nros-c-generated)
    # Add nros-c include directories for user code
    zephyr_include_directories(${NROS_REPO_DIR}/packages/core/nros-c/include)

    # Define platform macro so nros/platform.h selects the Zephyr backend
    zephyr_compile_definitions(NROS_PLATFORM_ZEPHYR)

    # Make nros_generate_interfaces() + nros_find_interfaces() available
    # for C examples. nros_find_interfaces() is the package.xml SSoT entry
    # point (issue #11) — it resolves the transitive interface closure and
    # delegates to the Zephyr emit-into-`app` nros_generate_interfaces.
    include(${CMAKE_CURRENT_LIST_DIR}/cmake/nros_generate_interfaces.cmake)
    # nros_find_interfaces() comes from the shared core (Phase 246), pulled in by
    # the generator's include above — no separate zephyr copy.

    # Select Cargo features based on RMW backend
    if(CONFIG_NROS_RMW_ZENOH)
        set(_nros_features "rmw-cffi,cffi-zenoh-cffi,platform-zephyr,ros-humble")
    elseif(CONFIG_NROS_RMW_XRCE)
        # Phase 115.K.2.5.1.3-zephyr — XRCE now routes through the C
        # backend's cffi shim instead of the deleted Rust direct-impl.
        # `cffi-xrce-c` flips on the auto-register hook inside
        # `nros_support_init` (mirrors the cyclonedds wiring); the
        # cffi shim's `platform-zephyr` feature drops every POSIX TU
        # and emits `UCLIENT_PLATFORM_ZEPHYR` in the generated config.
        set(_nros_features "rmw-cffi,cffi-xrce-c,platform-zephyr,ros-humble")
    elseif(CONFIG_NROS_RMW_CYCLONEDDS)
        # Phase 168.X gap 2.B — cyclonedds is now usable from the C
        # API too. The backend's `nros_rmw_cyclonedds_register`
        # entry point is declared with C linkage in
        # `packages/dds/nros-rmw-cyclonedds/include/nros_rmw_cyclonedds.h`,
        # so the Phase 160.A strong stub below resolves it from the
        # standalone C++ library compiled by the RMW backend block
        # above (lines 196+). nros-c just needs the generic cffi
        # runtime + Zephyr platform feature.
        set(_nros_features "rmw-cffi,platform-zephyr,ros-humble")
    endif()

    # native_sim targets run on Linux and support std (needed for panic handler)
    if(CONFIG_BOARD_NATIVE_SIM OR CONFIG_BOARD_NATIVE_POSIX)
        string(APPEND _nros_features ",std")
    endif()

    # Build nros-c static library via Cargo
    nros_cargo_build(
        PACKAGE nros-c
        FEATURES "${_nros_features}"
    )
    add_dependencies(app nros_c_cargo_build)

    # Link the built library
    zephyr_library_link_libraries(nros_c_cargo)

    # Phase 168.4 — Phase 134.fix dropped `dep:nros-rmw-zenoh` and
    # `dep:nros-rmw-dds` from `nros-c`'s feature graph so `libnros_c.a`
    # no longer carries `nros_rmw_<x>_register` for those backends.
    # The Phase 160.A strong stub below still references those symbols,
    # so the C-API Zephyr link fails for zenoh + dds without a
    # standalone backend staticlib. XRCE keeps `dep:nros-rmw-xrce-cffi`
    # under `cffi-xrce-c`, so it does NOT need a separate staticlib.
    if(CONFIG_NROS_RMW_ZENOH)
        set(_nros_backend_features "platform-zephyr")
        if(CONFIG_BOARD_NATIVE_SIM OR CONFIG_BOARD_NATIVE_POSIX)
            string(APPEND _nros_backend_features ",std")
        endif()
        nros_cargo_build(
            PACKAGE nros-rmw-zenoh-staticlib
            FEATURES "${_nros_backend_features}"
        )
        add_dependencies(app nros_rmw_zenoh_staticlib_cargo_build)
        zephyr_library_link_libraries(nros_rmw_zenoh_staticlib_cargo)
    endif()
    # Phase 169.4 retired dust-dds; the Rust DDS path (and its
    # `nros-rmw-dds-staticlib`) is gone. Future Cyclone DDS Rust
    # support comes via Phase 169.5's `nros-rmw-cyclonedds-sys`
    # shim — wire it in here when it lands.

    # Phase 160.A — emit the `nros_app_register_backends` strong stub
    # that calls the active backend's `nros_rmw_<x>_register` entry.
    # `nros_support_init` invokes this hook explicitly (Phase 155.B.4)
    # because `linkme`'s distributed-slice ctor doesn't fire on Zephyr.
    # Without the strong def, the weak no-op (shipped by
    # nros-c/c-stubs/weak_register_backends.c) wins → zero backends
    # register → `default_vtable() -> InvalidArgument` → C-API caller
    # sees `nros_support_init_named(...) -> -3`. The `nano_ros_link_rmw()`
    # cmake helper emits the equivalent stub for `add_subdirectory`
    # consumers; Zephyr needs its own emission point since
    # `add_subdirectory(<repo>)` isn't on the Zephyr consumption
    # shape.
    if(CONFIG_NROS_RMW_XRCE)
        set(_nros_rmw_name "xrce")
    elseif(CONFIG_NROS_RMW_ZENOH)
        set(_nros_rmw_name "zenoh")
    elseif(CONFIG_NROS_RMW_CYCLONEDDS)
        set(_nros_rmw_name "cyclonedds")
    endif()
    if(_nros_rmw_name)
        set(_nros_app_register_stub "${CMAKE_BINARY_DIR}/nros_app_register_backends.c")
        file(WRITE "${_nros_app_register_stub}"
"/* Phase 160.A auto-generated by zephyr/CMakeLists.txt — strong def\n\
 * of nros_app_register_backends() overrides the weak no-op in\n\
 * libnros_c_weak_stubs (Phase 104.B.6). Calls the active RMW backend's\n\
 * register entry inside `nros_support_init` (Phase 155.B.4 explicit\n\
 * hook for RTOS targets where linkme distributed-slice ctor doesn't\n\
 * fire). Backend registered: ${_nros_rmw_name}\n\
 */\n\
extern int nros_rmw_${_nros_rmw_name}_register(void);\n\
void nros_app_register_backends(void) {\n\
    (void)nros_rmw_${_nros_rmw_name}_register();\n\
}\n")
        zephyr_library_sources("${_nros_app_register_stub}")
    endif()
endif()

# =============================================================================
# C++ API path: build nros-cpp via Cargo and link it
# =============================================================================

if(CONFIG_NROS_CPP_API)
    # Phase 119.3: prepend per-build variant header dirs (cpp + c) to
    # include path before source-tree dirs. C++ user code transitively
    # pulls `<nros/nros_config_generated.h>` (C-side) via parameter.hpp,
    # so both variants must resolve to the per-build versions.
    zephyr_include_directories(${CMAKE_BINARY_DIR}/nros-rust/nros-cpp-generated)
    zephyr_include_directories(${CMAKE_BINARY_DIR}/nros-rust/nros-c-generated)
    # `cxx-compat/` ships a stripped-down `<cstdlib>` / `<cstdio>` /
    # `<cstring>` / etc. for Zephyr targets whose C++ stdlib layer is
    # minimal. `zephyr/lib/cpp/minimal/include` only carries
    # `<cstddef>`, `<cstdint>`, `<new>` — `<cstdio>` is missing on
    # `native_sim` too (Phase 160.A surfaced this for the XRCE C++
    # examples that `#include <nros/log.hpp>` → `<cstdio>`). The
    # original PICOLIBC gate was over-conservative: every Zephyr cpp
    # path needs these shims when the SDK's libstdc++ headers aren't
    # in the include set. Newlib targets that DO ship full libstdc++
    # would prefer their `<cstdio>` over the shim, but the shim's
    # `using ::fprintf;` re-export pattern is benign on top of a real
    # `<cstdio>`.
    zephyr_include_directories(${NROS_REPO_DIR}/zephyr/cxx-compat)
    # Add nros-cpp include directories for user code
    zephyr_include_directories(${NROS_REPO_DIR}/packages/core/nros-cpp/include)
    # Phase 112.C — nros-c also exposes `<nros/app_main.h>` (the
    # unified user-entry contract used by both C and C++ examples).
    # Cpp Zephyr builds need the nros-c include dir for that header
    # plus the Zephyr Kconfig synthesis variant of `<nros/app_config.h>`
    # under `nros/zephyr/`.
    zephyr_include_directories(${NROS_REPO_DIR}/packages/core/nros-c/include)
    zephyr_include_directories(${NROS_REPO_DIR}/packages/core/nros-c/include/nros/zephyr)

    # Make nros_generate_interfaces() + nros_find_interfaces() available
    # for C++ examples (see the C-API path above for the rationale).
    include(${CMAKE_CURRENT_LIST_DIR}/cmake/nros_generate_interfaces.cmake)
    # nros_find_interfaces() comes from the shared core (Phase 246), pulled in by
    # the generator's include above — no separate zephyr copy.

    # Select Cargo features based on RMW backend
    if(CONFIG_NROS_RMW_ZENOH)
        zephyr_compile_definitions(NROS_RMW_ZENOH_CFFI=1)
        set(_nros_cpp_features "rmw-zenoh-cffi,platform-zephyr,ros-humble")
    elseif(CONFIG_NROS_RMW_XRCE)
        zephyr_compile_definitions(NROS_RMW_XRCE=1)
        # Phase 115.K.2.5.1.3-zephyr — same shape as the C path
        # above: cffi runtime + C backend + Zephyr platform feature.
        set(_nros_cpp_features "rmw-cffi,rmw-xrce-cffi,platform-zephyr,ros-humble")
    elseif(CONFIG_NROS_RMW_CYCLONEDDS)
        # cyclonedds bypasses Cargo: the standalone nros-rmw-cyclonedds
        # C++ lib (compiled into the Zephyr app library by the RMW
        # backend block above) registers the vtable via
        # nros_rmw_cyclonedds_register(). nros-cpp only needs the
        # generic cffi runtime + Zephyr platform feature.
        set(_nros_cpp_features "rmw-cffi,platform-zephyr,ros-humble")
    endif()

    # native_sim targets use the host Rust target; keep std enabled so
    # the final native_sim executable links Rust's panic personality.
    if(CONFIG_BOARD_NATIVE_SIM OR CONFIG_BOARD_NATIVE_POSIX)
        string(APPEND _nros_cpp_features ",std")
    endif()

    # Phase 168.X gap 1 — `nros_log_emit` + `nros_log_emit_fmt`
    # symbols (the C-side log glue every `NROS_LOG_*` macro from
    # `<nros/log.h>` expands to) ship only with `libnros_c.a`.
    # `libnros_cpp.a` doesn't carry them, so C++-only Zephyr builds
    # (`CONFIG_NROS_C_API=n + CONFIG_NROS_CPP_API=y`) fail to link
    # the moment a cpp example uses `NROS_LOG_INFO`. Build nros-c
    # alongside nros-cpp (with the SAME RMW + platform feature set
    # the C-API path uses) so the log glue lands in the link line.
    # No code duplication: the C / C++ rlibs share their nros-node /
    # nros / nros-platform dep graph; the second cargo build wraps
    # them in nros-c's exported C symbols (init / log / executor /
    # …) — which is what nros-cpp's C++ classes call into anyway.
    #
    # Must run BEFORE the nros-cpp build below so the
    # nros-c-generated header byproducts are declared on the
    # nros_c_cargo_build target (nros-cpp's target then defers to it
    # via `TARGET nros_c_cargo_build` in nros_cargo_build.cmake).
    if(NOT CONFIG_NROS_C_API)
        if(CONFIG_NROS_RMW_ZENOH)
            set(_nros_c_for_cpp_features "rmw-cffi,cffi-zenoh-cffi,platform-zephyr,ros-humble")
        elseif(CONFIG_NROS_RMW_XRCE)
            set(_nros_c_for_cpp_features "rmw-cffi,cffi-xrce-c,platform-zephyr,ros-humble")
        elseif(CONFIG_NROS_RMW_CYCLONEDDS)
            set(_nros_c_for_cpp_features "rmw-cffi,platform-zephyr,ros-humble")
        endif()
        if(CONFIG_BOARD_NATIVE_SIM OR CONFIG_BOARD_NATIVE_POSIX)
            string(APPEND _nros_c_for_cpp_features ",std")
        endif()
        nros_cargo_build(
            PACKAGE nros-c
            FEATURES "${_nros_c_for_cpp_features}"
        )
        add_dependencies(app nros_c_cargo_build)
    endif()

    # Build nros-cpp static library via Cargo
    nros_cargo_build(
        PACKAGE nros-cpp
        FEATURES "${_nros_cpp_features}"
    )
    add_dependencies(app nros_cpp_cargo_build)

    # phase-263 C2c-zephyr — a MIXED workspace (C + C++ + Rust node) bundles nros-cpp + every
    # workspace Rust node into the synthesised `nros_ws_runtime` umbrella staticlib (the
    # single-runtime invariant: ONE Rust staticlib, one nros-rmw-cffi REGISTRY — a node in its
    # own staticlib would split it). The entry CMakeLists sets NROS_WS_RUST_NODE_DIRS (a list of
    # node pkg dirs) BEFORE find_package(Zephyr). nros-c / nros-cpp above are still BUILT (their
    # build.rs writes the per-build headers the C/C++ TUs include) but NOT linked — the umbrella
    # (which deps nros-cpp, hence nros-c) is the single linked archive, so the node libs' nros
    # refs resolve from it without double-defining nros-rmw-cffi's REGISTRY. Mirrors the
    # cmake/Corrosion lane's archive swap (NanoRosRuntimeCrate.cmake).
    set(_nros_link_umbrella FALSE)
    if(NROS_WS_RUST_NODE_DIRS)
        set(_nros_link_umbrella TRUE)
    endif()

    if(_nros_link_umbrella)
        include("${NROS_REPO_DIR}/cmake/NanoRosRuntimeCrate.cmake")
        set(NANO_ROS_ROOT "${NROS_REPO_DIR}")
        # Backend key for the umbrella's force-link anchor (zenoh/xrce bundle a Rust rlib).
        if(CONFIG_NROS_RMW_XRCE)
            set(_umb_backend "xrce")
        elseif(CONFIG_NROS_RMW_CYCLONEDDS)
            set(_umb_backend "cyclone")
        else()
            set(_umb_backend "zenoh")
        endif()
        # native_sim uses the host std triple → std umbrella; a real Zephyr board is no_std.
        set(_umb_no_std TRUE)
        if(CONFIG_BOARD_NATIVE_SIM OR CONFIG_BOARD_NATIVE_POSIX)
            set(_umb_no_std FALSE)
        endif()
        string(REPLACE "," ";" _umb_feats "${_nros_cpp_features}")
        nros_write_runtime_umbrella_crate(
            OUT_DIR      "${CMAKE_BINARY_DIR}/nros_ws_runtime"
            BACKEND      "${_umb_backend}"
            NO_STD       "${_umb_no_std}"
            NODE_DIRS    ${NROS_WS_RUST_NODE_DIRS}
            CPP_FEATURES ${_umb_feats})
        nros_cargo_build(
            PACKAGE       nros_ws_runtime
            MANIFEST_PATH "${CMAKE_BINARY_DIR}/nros_ws_runtime/Cargo.toml"
        )
        # Headers (nros-c / nros-cpp build.rs byproducts) must be ready before the umbrella
        # build's TUs / the entry TU compile.
        add_dependencies(nros_ws_runtime_cargo_build nros_cpp_cargo_build)
        if(TARGET nros_c_cargo_build)
            add_dependencies(nros_ws_runtime_cargo_build nros_c_cargo_build)
        endif()
        add_dependencies(app nros_ws_runtime_cargo_build)
        zephyr_library_link_libraries(nros_ws_runtime_cargo)
    else()
        # Link nros-c (log glue) + nros-cpp directly (pure-C / pure-C++ / pure-Rust entries).
        if(NOT CONFIG_NROS_C_API AND TARGET nros_c_cargo)
            zephyr_library_link_libraries(nros_c_cargo)
        endif()
        zephyr_library_link_libraries(nros_cpp_cargo)
    endif()

    # Phase 168.4 — same backend staticlib link as the C-API branch
    # above. `nros-cpp`'s `rmw-zenoh-cffi` feature does NOT pull
    # `dep:nros-rmw-zenoh` (Phase 134.fix), so the Phase 160.A strong
    # stub below references `nros_rmw_zenoh_register` with no provider
    # unless we link the standalone backend staticlib. `rmw-xrce-cffi`
    # still carries its dep, so it doesn't need this extra step.
    if(CONFIG_NROS_RMW_ZENOH)
        set(_nros_cpp_backend_features "platform-zephyr")
        if(CONFIG_BOARD_NATIVE_SIM OR CONFIG_BOARD_NATIVE_POSIX)
            string(APPEND _nros_cpp_backend_features ",std")
        endif()
        if(NOT TARGET nros_rmw_zenoh_staticlib_cargo)
            nros_cargo_build(
                PACKAGE nros-rmw-zenoh-staticlib
                FEATURES "${_nros_cpp_backend_features}"
            )
        endif()
        add_dependencies(app nros_rmw_zenoh_staticlib_cargo_build)
        zephyr_library_link_libraries(nros_rmw_zenoh_staticlib_cargo)
    endif()

    # Phase 160.A — same register-stub emission as the C-API branch
    # above. C++ apps call `nros_cpp_init` which internally invokes
    # `nros_app_register_backends`; without the strong def the weak
    # no-op fires and `default_vtable() -> InvalidArgument` surfaces
    # as `nros_support_init_named(...) -> -3`.
    if(CONFIG_NROS_RMW_XRCE)
        set(_nros_cpp_rmw_name "xrce")
    elseif(CONFIG_NROS_RMW_ZENOH)
        set(_nros_cpp_rmw_name "zenoh")
    elseif(CONFIG_NROS_RMW_CYCLONEDDS)
        set(_nros_cpp_rmw_name "cyclonedds")
    endif()
    if(_nros_cpp_rmw_name)
        set(_nros_cpp_app_register_stub "${CMAKE_BINARY_DIR}/nros_app_register_backends.c")
        file(WRITE "${_nros_cpp_app_register_stub}"
"/* Phase 160.A auto-generated by zephyr/CMakeLists.txt (CPP API). */\n\
extern int nros_rmw_${_nros_cpp_rmw_name}_register(void);\n\
void nros_app_register_backends(void) {\n\
    (void)nros_rmw_${_nros_cpp_rmw_name}_register();\n\
}\n")
        zephyr_library_sources("${_nros_cpp_app_register_stub}")
    endif()
endif()

# =============================================================================
# Rust API path: bridge Kconfig → Cargo env vars
# =============================================================================

if(CONFIG_NROS_RUST_API)
    nros_set_cargo_env_from_kconfig()
endif()

endif() # CONFIG_NROS
