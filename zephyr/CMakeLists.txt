# nros Zephyr Module
# Copyright (c) 2024 nros contributors
# SPDX-License-Identifier: MIT OR Apache-2.0
#
# This module compiles backend-specific C sources centrally and bridges
# Kconfig values to Cargo environment variables, so individual examples
# don't need to manually include them.

if(CONFIG_NROS)

include(${CMAKE_CURRENT_LIST_DIR}/cmake/nros_cargo_build.cmake)

# The nros repo root (parent of this zephyr/ directory)
set(NROS_REPO_DIR ${CMAKE_CURRENT_LIST_DIR}/..)

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
zephyr_include_directories(${NROS_REPO_DIR}/packages/core/nros-platform-cffi/include)
zephyr_library_sources(
    ${NROS_REPO_DIR}/packages/core/nros-platform-zephyr/src/platform.c
    ${NROS_REPO_DIR}/packages/core/nros-platform-zephyr/src/net.c
    ${NROS_REPO_DIR}/packages/core/nros-platform-zephyr/src/timer.c
)

# =============================================================================
# Backend-specific C sources
# =============================================================================

if(CONFIG_NROS_RMW_ZENOH)
    # -------------------------------------------------------------------------
    # Zenoh-pico library (compiled from vendored submodule)
    # -------------------------------------------------------------------------

    # Vendored zenoh-pico submodule (single source of truth for all builds)
    set(ZENOH_PICO_DIR ${NROS_REPO_DIR}/packages/zpico/zpico-sys/zenoh-pico)

    # --- zenoh-pico sources ---

    file(GLOB_RECURSE _zenoh_pico_sources
        "${ZENOH_PICO_DIR}/src/api/*.c"
        "${ZENOH_PICO_DIR}/src/collections/*.c"
        "${ZENOH_PICO_DIR}/src/link/*.c"
        "${ZENOH_PICO_DIR}/src/net/*.c"
        "${ZENOH_PICO_DIR}/src/protocol/*.c"
        "${ZENOH_PICO_DIR}/src/session/*.c"
        "${ZENOH_PICO_DIR}/src/transport/*.c"
        "${ZENOH_PICO_DIR}/src/utils/*.c"
        "${ZENOH_PICO_DIR}/src/system/common/*.c"
    )
    # Zephyr platform: none of zenoh-pico's src/system/zephyr/*.c is compiled.
    # - system.c (clock/memory/sleep/random/threading/time) is replaced by
    #   nros-platform-zephyr → zpico-platform-shim.
    # - network.c (TCP/UDP/multicast) is replaced by nros-platform-zephyr's
    #   net.rs module (Phase 80 network unification) → zpico-platform-shim's
    #   network feature forwarders.
    # Both replacements link in via the Rust staticlib produced from
    # nros-c / nros-cpp / rustapp.

    zephyr_library_sources(${_zenoh_pico_sources})

    # zenoh-pico include directory
    zephyr_include_directories(${ZENOH_PICO_DIR}/include)

    # --- zenoh-pico compile definitions ---

    # Zephyr platform backend
    zephyr_compile_definitions(ZENOH_ZEPHYR)

    # Router-backed client-to-client routing requires interest declarations
    # so zenohd knows which peers should receive each keyexpr. Keep matching
    # callbacks disabled on Zephyr; they are not needed for routing and can
    # create high-rate executor wakeups.
    zephyr_compile_definitions(Z_FEATURE_INTEREST=1 Z_FEATURE_MATCHING=0)

    # Map NROS_ZENOH_* Kconfig options to Z_FEATURE_* compile definitions.
    # The function strips the CONFIG_NROS_ZENOH_ prefix and replaces it with
    # Z_FEATURE_, then sets =1 or =0 based on the Kconfig boolean value.
    function(_nros_configure_zenoh_feature config)
        string(REPLACE "CONFIG_NROS_ZENOH_" "Z_FEATURE_" feature "${config}")
        if(${config})
            zephyr_compile_definitions(${feature}=1)
        else()
            zephyr_compile_definitions(${feature}=0)
        endif()
    endfunction()

    _nros_configure_zenoh_feature(CONFIG_NROS_ZENOH_MULTI_THREAD)
    _nros_configure_zenoh_feature(CONFIG_NROS_ZENOH_PUBLICATION)
    _nros_configure_zenoh_feature(CONFIG_NROS_ZENOH_SUBSCRIPTION)
    _nros_configure_zenoh_feature(CONFIG_NROS_ZENOH_QUERY)
    _nros_configure_zenoh_feature(CONFIG_NROS_ZENOH_QUERYABLE)
    _nros_configure_zenoh_feature(CONFIG_NROS_ZENOH_LINK_TCP)
    _nros_configure_zenoh_feature(CONFIG_NROS_ZENOH_LINK_UDP_UNICAST)
    _nros_configure_zenoh_feature(CONFIG_NROS_ZENOH_LINK_UDP_MULTICAST)
    _nros_configure_zenoh_feature(CONFIG_NROS_ZENOH_SCOUTING)
    _nros_configure_zenoh_feature(CONFIG_NROS_ZENOH_LINK_SERIAL)
    _nros_configure_zenoh_feature(CONFIG_NROS_ZENOH_LINK_WS)
    _nros_configure_zenoh_feature(CONFIG_NROS_ZENOH_RAWETH_TRANSPORT)

    # -------------------------------------------------------------------------
    # Shared sources: zenoh shim + zpico-zephyr platform support
    # -------------------------------------------------------------------------

    # zpico.c — the C API layer over zenoh-pico
    zephyr_library_sources(
        ${NROS_REPO_DIR}/packages/zpico/zpico-sys/c/zpico/zpico.c
    )
    zephyr_library_sources(${CMAKE_CURRENT_LIST_DIR}/nros_zenoh_zephyr_system.c)

    # zpico_zephyr.c — Zephyr platform support (network wait, session init)
    zephyr_library_sources(
        ${NROS_REPO_DIR}/packages/zpico/zpico-zephyr/src/zpico_zephyr.c
    )

    # Include directories for zpico and platform headers
    zephyr_include_directories(${NROS_REPO_DIR}/packages/zpico/zpico-sys/c/include)
    zephyr_include_directories(${NROS_REPO_DIR}/packages/zpico/zpico-zephyr/include)

    # -------------------------------------------------------------------------
    # Transport tuning: Kconfig → C preprocessor flags
    # -------------------------------------------------------------------------

    # Map Kconfig transport tuning to ZPICO_* defines consumed by zpico.c
    zephyr_compile_definitions(
        ZPICO_MAX_PUBLISHERS=${CONFIG_NROS_MAX_PUBLISHERS}
        ZPICO_MAX_SUBSCRIBERS=${CONFIG_NROS_MAX_SUBSCRIBERS}
        ZPICO_MAX_QUERYABLES=${CONFIG_NROS_MAX_QUERYABLES}
        ZPICO_MAX_LIVELINESS=${CONFIG_NROS_MAX_LIVELINESS}
        ZPICO_MAX_PENDING_GETS=${CONFIG_NROS_MAX_PENDING_GETS}
        ZPICO_GET_REPLY_BUF_SIZE=${CONFIG_NROS_GET_REPLY_BUF_SIZE}
        ZPICO_GET_POLL_INTERVAL_MS=${CONFIG_NROS_GET_POLL_INTERVAL_MS}
        ZPICO_FRAG_MAX_SIZE=${CONFIG_NROS_FRAG_MAX_SIZE}
        ZPICO_BATCH_UNICAST_SIZE=${CONFIG_NROS_BATCH_UNICAST_SIZE}
    )

elseif(CONFIG_NROS_RMW_XRCE)
    # -------------------------------------------------------------------------
    # XRCE-DDS platform support (compiled by Zephyr CMake)
    # -------------------------------------------------------------------------
    # xrce_zephyr.c provides L4 network readiness wait + uxr_millis/uxr_nanos.

    zephyr_library_sources(
        ${NROS_REPO_DIR}/packages/xrce/xrce-zephyr/src/xrce_zephyr.c
    )
    zephyr_include_directories(
        ${NROS_REPO_DIR}/packages/xrce/xrce-zephyr/include
    )

elseif(CONFIG_NROS_RMW_DDS)
    # -------------------------------------------------------------------------
    # DDS (dust-dds) platform support — Phase 71.8
    # -------------------------------------------------------------------------
    # dust-dds itself is pure-Rust (no C shim), but the example wrapper
    # `nros::platform::zephyr::wait_for_network` calls `zpico_zephyr_wait_network`
    # — a generic L4-readiness helper that lives in the zpico-zephyr crate
    # but is actually backend-agnostic. Pull it in for DDS too.
    zephyr_library_sources(
        ${NROS_REPO_DIR}/packages/zpico/zpico-zephyr/src/zpico_zephyr.c
    )
    zephyr_include_directories(${NROS_REPO_DIR}/packages/zpico/zpico-zephyr/include)
    # zpico_zephyr.c #includes "zpico.h" for the session-init helpers we
    # don't use under DDS, but the header is still parsed; expose the
    # zpico-sys include dir so the compile succeeds.
    zephyr_include_directories(${NROS_REPO_DIR}/packages/zpico/zpico-sys/c/include)

elseif(CONFIG_NROS_RMW_CYCLONEDDS)
    # -------------------------------------------------------------------------
    # Cyclone DDS RMW backend — Phase 117 + ASI Phase 1D
    # -------------------------------------------------------------------------
    # Compiles the pinned Cyclone DDS submodule (tag 0.10.5) plus the
    # standalone nros-rmw-cyclonedds C++ register glue directly into the
    # Zephyr app library. No nros-rmw-cyclonedds CMake subproject step —
    # the standalone project at packages/dds/nros-rmw-cyclonedds/ is a
    # POSIX-host build path; we replicate the relevant compile here.
    #
    # Foundation only. Iteration items deferred to first real build:
    #   - platform-shim audit (getifaddrs, pthread_setname_np, IGMP) in
    #     nros_platform_zephyr_shims.c
    #   - L4 wait_for_network hookup (zpico_zephyr helper, like dust-dds)
    #   - link-time symbol gap fixes
    #   - ROM/RAM trimming for S32Z

    set(CYCLONEDDS_DIR ${NROS_REPO_DIR}/third-party/dds/cyclonedds)
    set(NROS_RMW_CDDS_DIR ${NROS_REPO_DIR}/packages/dds/nros-rmw-cyclonedds)

    if(NOT EXISTS ${CYCLONEDDS_DIR}/src/ddsrt/include/dds/config.h.in)
        message(FATAL_ERROR
            "Cyclone DDS submodule not initialised at ${CYCLONEDDS_DIR}. "
            "Run: git submodule update --init third-party/dds/cyclonedds")
    endif()

    # ---- dds/config.h shim --------------------------------------------------
    # Cyclone's CMake normally generates dds/config.h via configure_file()
    # after probing the host toolchain. Zephyr cross-builds can't run those
    # probes, so a hand-rolled shim ships under zephyr/cyclonedds-config/.
    # It is included by Cyclone source files via the standard "dds/config.h"
    # path, so prepend our shim dir to the include search path.
    zephyr_include_directories(${CMAKE_CURRENT_LIST_DIR}/cyclonedds-config)

    # Force-include a Zephyr IPv4 compat header on every C/C++ TU
    # (NOT .S assembly — `-include` of a C header into asm
    # breaks). Provides `struct ip_mreq` / `ip_mreqn` where Cyclone
    # references them directly. Actual multicast joins go through
    # the IGMP API in nros-platform-zephyr/src/net.c.
    zephyr_compile_options(
        $<$<OR:$<COMPILE_LANGUAGE:C>,$<COMPILE_LANGUAGE:CXX>>:SHELL:-include ${CMAKE_CURRENT_LIST_DIR}/cyclonedds-config/zephyr_ipv4_compat.h>
    )

    # ---- Cyclone DDS sources -----------------------------------------------
    # ddsrt: platform-abstraction layer. Top-level .c files plus per-feature
    # POSIX backends (sockets / threads / sync / time / heap / etc.).
    file(GLOB _cdds_ddsrt_top   ${CYCLONEDDS_DIR}/src/ddsrt/src/*.c)
    file(GLOB _cdds_ddsrt_posix ${CYCLONEDDS_DIR}/src/ddsrt/src/*/posix/*.c)

    # Drop POSIX TUs that reference symbols Zephyr does NOT provide. The
    # `DDSRT_HAVE_*` undef'd in cyclonedds-config/dds/config.h only gate
    # *call sites* in headers + top-level TUs; the per-feature posix/*.c
    # bodies are still unconditional and would fail to link.
    #   - ifaddrs/posix/ifaddrs.c    → getifaddrs / struct ifaddrs (absent)
    #   - dynlib/posix/dynlib.c      → dlopen / dlsym / dlerror (no libdl)
    #   - random/posix/random.c      → fopen("/dev/urandom") — replaced by
    #     the Zephyr-specific ddsrt_prng_makeseed override under
    #     zephyr/cyclonedds-zephyr/random_zephyr.c.
    #   - sockets/posix/gethostname.c → empty under DDSRT_HAVE_GETHOSTNAME=0
    #     but drop for clarity.
    #   - filesystem/posix/filesystem.c → DDSRT_HAVE_FILESYSTEM=0 in config.h
    #     but the body references DDSRT_FILESEPCHAR / ddsrt_dir_handle_t
    #     unconditionally; drop to avoid build errors.
    list(REMOVE_ITEM _cdds_ddsrt_posix
        ${CYCLONEDDS_DIR}/src/ddsrt/src/ifaddrs/posix/ifaddrs.c
        ${CYCLONEDDS_DIR}/src/ddsrt/src/dynlib/posix/dynlib.c
        ${CYCLONEDDS_DIR}/src/ddsrt/src/random/posix/random.c
        ${CYCLONEDDS_DIR}/src/ddsrt/src/sockets/posix/gethostname.c
        ${CYCLONEDDS_DIR}/src/ddsrt/src/filesystem/posix/filesystem.c
    )

    # Zephyr replacement TUs for the dropped POSIX bodies above.
    set(_cdds_zephyr_overrides
        ${CMAKE_CURRENT_LIST_DIR}/cyclonedds-zephyr/random_zephyr.c
    )

    # ddsi: protocol engine. ddsc: public C API. `.part.c` files
    # are partials meant to be #include'd from sibling TUs, not
    # compiled directly — drop them from the source list.
    file(GLOB _cdds_ddsi ${CYCLONEDDS_DIR}/src/core/ddsi/src/*.c)
    file(GLOB _cdds_ddsc ${CYCLONEDDS_DIR}/src/core/ddsc/src/*.c)
    list(FILTER _cdds_ddsi EXCLUDE REGEX "\\.part\\.c$")
    list(FILTER _cdds_ddsc EXCLUDE REGEX "\\.part\\.c$")
    # Security TUs depend on dds/security/core/* headers that come from
    # the security plugin tree we don't compile (DDS_HAS_SECURITY=0).
    # Drop matching ddsi sources.
    list(FILTER _cdds_ddsi EXCLUDE REGEX "ddsi_(security|handshake|omg_security)[^/]*\\.c$")
    # ddsi_vnet.c (virtual-network locator transport) uses POSIX
    # `struct sockaddr::sa_data` which Zephyr's `struct sockaddr`
    # renames to `data`. We don't use vnet transport on the embedded
    # path; drop the TU.
    list(FILTER _cdds_ddsi EXCLUDE REGEX "ddsi_vnet\\.c$")

    zephyr_library_sources(
        ${_cdds_ddsrt_top}
        ${_cdds_ddsrt_posix}
        ${_cdds_zephyr_overrides}
        ${_cdds_ddsi}
        ${_cdds_ddsc}
    )

    zephyr_include_directories(
        ${CYCLONEDDS_DIR}/src/ddsrt/include
        ${CYCLONEDDS_DIR}/src/core/include
        ${CYCLONEDDS_DIR}/src/core/ddsc/include
        ${CYCLONEDDS_DIR}/src/core/ddsi/include
        # Private headers (sockets_priv.h, threads_priv.h) live alongside
        # their corresponding .c files in ddsrt's src/ tree and are
        # included via `"name.h"` from sibling TUs. Expose the dir.
        ${CYCLONEDDS_DIR}/src/ddsrt/src
        # Private headers in ddsc src tree (dds__alloc.h, dds__entity.h, ...)
        ${CYCLONEDDS_DIR}/src/core/ddsc/src
    )

    # Trim Cyclone optional features that drag in host-toolchain deps.
    zephyr_compile_definitions(
        DDS_HAS_NETWORK_PARTITIONS=0
        DDS_HAS_SECURITY=0
        DDS_HAS_SHM=0
        DDS_HAS_TYPE_DISCOVERY=0
        DDS_HAS_TOPIC_DISCOVERY=0
    )

    # Zephyr POSIX doesn't expose `INADDR_LOOPBACK`. Cyclone's
    # `ddsrt/src/sockets.c` uses it unconditionally for IPv4
    # loopback compare. Provide the canonical value via -D so
    # the TU compiles without patching upstream.
    zephyr_compile_definitions(INADDR_LOOPBACK=0x7f000001UL)

    # More Zephyr POSIX gaps Cyclone hits unconditionally:
    #   - MAXTHREADNAMESIZE: pthread_setname_np max bytes (Linux=16, BSD=64).
    #   - SIG_BLOCK / SIG_SETMASK: pthread_sigmask flags. Zephyr's
    #     POSIX_SIGNAL_GROUP defines them; if not built in, fall back
    #     to canonical Linux values (no-op at runtime since
    #     pthread_sigmask is a no-op stub under Zephyr without signals).
    #   - IFF_UP / IFF_LOOPBACK / IFF_MULTICAST: net iface flags from
    #     <net/if.h>. Cyclone's ddsi_ownip.c references them; provide
    #     canonical Linux values.
    zephyr_compile_definitions(
        MAXTHREADNAMESIZE=64
        IFF_UP=0x1
        IFF_LOOPBACK=0x8
        IFF_POINTOPOINT=0x10
        IFF_MULTICAST=0x1000
        # POSIX signal flags absent from Zephyr's minimal POSIX.
        # pthread_sigmask is a no-op stub under Zephyr without signals,
        # so the value doesn't matter at runtime; just need to compile.
        SIG_BLOCK=0
        SIG_UNBLOCK=1
        SIG_SETMASK=2
        # Multicast socket-option constants. Zephyr's zsock layer ≤3.5
        # doesn't expose these via <zephyr/posix/sys/socket.h>; values
        # match Linux. ddsi_udp.c's setsockopt calls will fail at
        # runtime (Zephyr returns -ENOPROTOOPT) — Cyclone treats those
        # failures as non-fatal warnings on most paths. For actual SPDP
        # multicast joins, use the Cyclone XML's <General><MulticastRecvNetworkInterfaceAddresses>
        # or fall back to the nros-platform-zephyr IGMP path.
        IP_MULTICAST_IF=32
        IP_MULTICAST_TTL=33
        IP_MULTICAST_LOOP=34
        IP_ADD_MEMBERSHIP=35
        IP_DROP_MEMBERSHIP=36
    )

    # ---- nros-rmw-cyclonedds C++ register glue ------------------------------
    file(GLOB _nros_cdds_cpp ${NROS_RMW_CDDS_DIR}/src/*.cpp)
    zephyr_library_sources(${_nros_cdds_cpp})
    zephyr_include_directories(
        ${NROS_RMW_CDDS_DIR}/include
        ${NROS_RMW_CDDS_DIR}/src
        ${NROS_REPO_DIR}/packages/core/nros-rmw-cffi/include
    )

    # Flips the auto-register hook in packages/core/nros-cpp/include/nros/node.hpp
    # which calls nros_rmw_cyclonedds_register() inside nros::init().
    zephyr_compile_definitions(
        NROS_RMW_CYCLONEDDS=1
        NROS_CYCLONE_DOMAIN_ID=${CONFIG_NROS_CYCLONE_DOMAIN_ID}
    )

    # L4-readiness helper (same backend-agnostic helper dust-dds uses).
    zephyr_library_sources(${NROS_REPO_DIR}/packages/zpico/zpico-zephyr/src/zpico_zephyr.c)
    zephyr_include_directories(
        ${NROS_REPO_DIR}/packages/zpico/zpico-zephyr/include
        ${NROS_REPO_DIR}/packages/zpico/zpico-sys/c/include
    )

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

    # Make nros_generate_interfaces() available for C examples
    include(${CMAKE_CURRENT_LIST_DIR}/cmake/nros_generate_interfaces.cmake)

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
    elseif(CONFIG_NROS_RMW_DDS)
        set(_nros_features "rmw-cffi,cffi-dds-cffi,platform-zephyr,ros-humble")
    elseif(CONFIG_NROS_RMW_CYCLONEDDS)
        # cyclonedds backend has no Cargo-side shim crate — the C++
        # nros-rmw-cyclonedds lib registers via the C++ auto-hook in
        # nros-cpp's node.hpp. C API path is not yet supported here;
        # the Kconfig guard above (`depends on NROS_CPP_API`) prevents
        # this combo, and this branch errors as a belt-and-braces
        # check in case the guard is bypassed.
        message(FATAL_ERROR
            "CONFIG_NROS_C_API=y is not supported with "
            "CONFIG_NROS_RMW_CYCLONEDDS=y. Use CONFIG_NROS_CPP_API=y "
            "or pick a different RMW backend.")
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
    # `cxx-compat/` ships a stripped-down `<cstdlib>` (and friends)
    # for picolibc-based Zephyr targets where the toolchain's full
    # libstdc++ C-shim layer isn't wired up. Injecting it on newlib
    # targets (e.g. zephyr-sdk-0.16.3 aarch64-zephyr-elf) shadows
    # the real `<cstdlib>` and breaks every `std::` symbol the shim
    # doesn't explicitly re-export (`abort`, `exit`, `strtol`,
    # `malloc`, `free`, ...). Gate on CONFIG_PICOLIBC so newlib
    # builds see the proper header.
    if(CONFIG_PICOLIBC)
        zephyr_include_directories(${NROS_REPO_DIR}/zephyr/cxx-compat)
    endif()
    # Add nros-cpp include directories for user code
    zephyr_include_directories(${NROS_REPO_DIR}/packages/core/nros-cpp/include)
    # Phase 112.C — nros-c also exposes `<nros/app_main.h>` (the
    # unified user-entry contract used by both C and C++ examples).
    # Cpp Zephyr builds need the nros-c include dir for that header
    # plus the Zephyr Kconfig synthesis variant of `<nros/app_config.h>`
    # under `nros/zephyr/`.
    zephyr_include_directories(${NROS_REPO_DIR}/packages/core/nros-c/include)
    zephyr_include_directories(${NROS_REPO_DIR}/packages/core/nros-c/include/nros/zephyr)

    # Make nros_generate_interfaces() available for C++ examples
    include(${CMAKE_CURRENT_LIST_DIR}/cmake/nros_generate_interfaces.cmake)

    # Select Cargo features based on RMW backend
    if(CONFIG_NROS_RMW_ZENOH)
        zephyr_compile_definitions(NROS_RMW_ZENOH_CFFI=1)
        set(_nros_cpp_features "rmw-zenoh-cffi,platform-zephyr,ros-humble")
    elseif(CONFIG_NROS_RMW_XRCE)
        zephyr_compile_definitions(NROS_RMW_XRCE=1)
        # Phase 115.K.2.5.1.3-zephyr — same shape as the C path
        # above: cffi runtime + C backend + Zephyr platform feature.
        set(_nros_cpp_features "rmw-cffi,rmw-xrce-cffi,platform-zephyr,ros-humble")
    elseif(CONFIG_NROS_RMW_DDS)
        zephyr_compile_definitions(NROS_RMW_DDS_CFFI=1)
        set(_nros_cpp_features "rmw-dds-cffi,platform-zephyr,ros-humble")
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

    # Build nros-cpp static library via Cargo
    nros_cargo_build(
        PACKAGE nros-cpp
        FEATURES "${_nros_cpp_features}"
    )
    add_dependencies(app nros_cpp_cargo_build)

    # Link the built library
    zephyr_library_link_libraries(nros_cpp_cargo)
endif()

# =============================================================================
# Rust API path: bridge Kconfig → Cargo env vars
# =============================================================================

if(CONFIG_NROS_RUST_API)
    nros_set_cargo_env_from_kconfig()
endif()

endif() # CONFIG_NROS
