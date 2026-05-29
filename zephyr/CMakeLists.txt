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
    # Phase 200.1 — RMW-independent network-wait (compiled for every RMW; was
    # mis-filed as zpico_zephyr_wait_network in the zenoh-only zpico_zephyr.c).
    ${NROS_REPO_DIR}/packages/core/nros-platform-zephyr/src/net_wait.c
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
    # Zephyr platform: system.c (clock/memory/sleep/random/threading/time)
    # is replaced by the alias TU (`packages/zpico/zpico-sys/c/zpico/
    # platform_aliases.c`) compiled inside the cargo-built Rust
    # staticlib (`librustapp.a` / `libnros_c.a`). That TU forwards each
    # `_z_*` to the canonical `nros_platform_*` ABI provided by
    # `nros-platform-zephyr`. Phase 129 retired `zpico-platform-shim`;
    # the alias TU is the single replacement provider.
    #
    # Phase 160.C — network.c (TCP/UDP/multicast) MUST come from
    # zenoh-pico's `src/system/zephyr/network.c`, NOT from the alias
    # TU. The alias TU's `_z_open_tcp` / `_z_send_tcp` / etc. see a
    # generic 32-byte `_z_sys_net_socket_t` opaque (from
    # `nros_zenoh_generic_platform.h`); the Zephyr-side `tx.c` /
    # `link.c` (compiled here under `ZENOH_ZEPHYR`) see the
    # 4-byte `{int _fd}` socket from `system/platform/zephyr.h`.
    # The endpoint layouts diverge too (alias 16 B opaque vs.
    # `{struct addrinfo*}` 8 B). The size mismatch propagates
    # through the by-value endpoint arg and the by-pointer socket
    # arg, corrupting the connect-time state → `Transport(
    # ConnectionFailed)` on every Zephyr Rust app at session open.
    # Same family as Phase 159 NuttX (`_z_send_tcp` ABI gate). The
    # paired build.rs change extends `NROS_ZENOH_PLATFORM_USES_UNIX`
    # to zephyr so the alias TU's network section is `#ifndef`-elided
    # at cargo compile time — without that, both providers land and
    # the alias version wins under `--allow-multiple-definition`.
    zephyr_library_sources(${_zenoh_pico_sources}
        "${ZENOH_PICO_DIR}/src/system/zephyr/network.c")

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

    # Force-include a Zephyr IPv4 compat header (NOT .S assembly — `-include`
    # of a C header into asm breaks). Provides `struct ip_mreq` where Cyclone
    # references it directly. Actual multicast joins go through the IGMP API
    # in nros-platform-zephyr/src/net.c.
    #
    # Phase 180.A — on Zephyr 4.x scope this to the nros library (where the
    # cyclonedds TUs live), NOT globally. A global zephyr_compile_options
    # force-include also lands on Zephyr's own heap_constants.c bootstrap TU,
    # whose 4.x kernel.h pulls the not-yet-generated <zephyr/heap_constants.h>
    # (chicken-and-egg: that TU is compiled to generate the header). 3.7's
    # kernel.h has no such include, so the global form is harmless there —
    # kept to avoid disturbing the LTS line.
    set(_nros_cdds_ipv4_compat
        "$<$<OR:$<COMPILE_LANGUAGE:C>,$<COMPILE_LANGUAGE:CXX>>:SHELL:-include ${CMAKE_CURRENT_LIST_DIR}/cyclonedds-config/zephyr_ipv4_compat.h>")
    if(Zephyr_VERSION VERSION_GREATER_EQUAL 4.0)
        target_compile_options(nros PRIVATE ${_nros_cdds_ipv4_compat})
    else()
        zephyr_compile_options(${_nros_cdds_ipv4_compat})
    endif()

    # Phase 11W.1 — Cyclone DDS atomics use unprefixed `asm
    # volatile` in `ddsrt/atomics/gcc.h`. Zephyr's default -std=c11
    # rejects the unprefixed keyword on strict toolchains
    # (host-gcc on native_sim). `-fgnu-keywords` is C++-only;
    # for C, the equivalent is a macro substitution to the
    # always-accepted `__asm__` form.
    zephyr_compile_options($<$<COMPILE_LANGUAGE:C>:-Dasm=__asm__>)

    # Phase 11W.2 — `packages/dds/nros-rmw-cyclonedds/src/*.cpp`
    # include `<cstdlib>` / `<cstring>` etc. Zephyr's
    # `lib/cpp/minimal/include` only ships `<cstddef>` / `<cstdint>`
    # / `<new>`. Project ships compat shims at `zephyr/cxx-compat/`
    # — same shims `nros-cpp` uses. Prepend so cyclonedds C++ TUs
    # find them.
    # Phase 190.L — ONLY when the C++ stdlib is minimal. With a full libstdc++
    # (CONFIG_GLIBCXX_LIBCPP, e.g. the Autoware safety-island FVP profile) the
    # real <atomic>/<chrono>/<utility>/<cstdlib>/… exist; putting cxx-compat on
    # the global -I then breaks libstdc++'s own internals (its <bits/move.h>
    # stops seeing std::remove_reference once the dir is searched), even with
    # the per-shim #include_next passthrough guards. Skip the dir entirely.
    if(NOT CONFIG_GLIBCXX_LIBCPP)
        zephyr_include_directories(${CMAKE_CURRENT_LIST_DIR}/cxx-compat)
    endif()

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
    #   - process/posix/process.c    → fopen("/proc/self/cmdline") in
    #     ddsrt_getprocessname(), which aborts through Zephyr fs_open()
    #     when no filesystem is mounted.
    #   - sockets/posix/gethostname.c → empty under DDSRT_HAVE_GETHOSTNAME=0
    #     but drop for clarity.
    #   - filesystem/posix/filesystem.c → DDSRT_HAVE_FILESYSTEM=0 in config.h
    #     but the body references DDSRT_FILESEPCHAR / ddsrt_dir_handle_t
    #     unconditionally; drop to avoid build errors.
    list(REMOVE_ITEM _cdds_ddsrt_posix
        ${CYCLONEDDS_DIR}/src/ddsrt/src/ifaddrs/posix/ifaddrs.c
        ${CYCLONEDDS_DIR}/src/ddsrt/src/dynlib/posix/dynlib.c
        ${CYCLONEDDS_DIR}/src/ddsrt/src/process/posix/process.c
        ${CYCLONEDDS_DIR}/src/ddsrt/src/random/posix/random.c
        ${CYCLONEDDS_DIR}/src/ddsrt/src/sockets/posix/gethostname.c
        ${CYCLONEDDS_DIR}/src/ddsrt/src/filesystem/posix/filesystem.c
    )

    # Zephyr replacement TUs for the dropped POSIX bodies above.
    set(_cdds_zephyr_overrides
        ${CMAKE_CURRENT_LIST_DIR}/cyclonedds-zephyr/random_zephyr.c
        ${CMAKE_CURRENT_LIST_DIR}/cyclonedds-zephyr/process_zephyr.c
        # SHM stub fns — original ddsi_shm_transport.c dropped under
        # DDS_HAS_SHM=0 but downstream headers still publicly DDS_EXPORT
        # iox_sub_context_*() / shm_{lock,unlock}_iox_sub(). Stub bodies
        # satisfy the linker; never called at runtime.
        ${CMAKE_CURRENT_LIST_DIR}/cyclonedds-zephyr/shm_stubs.c
        # Phase 171.0.c — Zephyr's current minimal C++ runtime now
        # defines the nothrow new/delete overloads itself, but leaves
        # the `std::nothrow` tag object unresolved on AArch64 FVP.
        # Keep a tag-only TU instead of the old Phase 11W.3 operator
        # override, which now collides with Zephyr's cpp_new.cpp.
        ${CMAKE_CURRENT_LIST_DIR}/cyclonedds-zephyr/nothrow_tag.cpp
        # Phase 11W.4 — link-time stubs for the residual unreferenced
        # symbols (ddsi_vnet_init / ddsrt_getifaddrs / IN_MULTICAST
        # macro define). Most of the original 88 undef-references
        # disappeared once `DDS_HAS_*` was switched from `=0` to
        # leave-undefined; only these three needed explicit stubs.
        ${CMAKE_CURRENT_LIST_DIR}/cyclonedds-zephyr/link_stubs.c
        # Phase 184.G — weak POSIX symbols the kept posix ddsrt backend
        # (sockets/posix/socket.c recvmsg; threads/posix/threads.c
        # pthread_attr_setscope/setinheritsched/pthread_sigmask) references but
        # that the 3.5/3.7-LTS Zephyr POSIX layer (FVP profile) lacks. Weak, so
        # a 4.x that provides real ones wins.
        ${CMAKE_CURRENT_LIST_DIR}/cyclonedds-zephyr/posix_compat_zephyr.c
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
    # ddsi_shm_transport.c is the SHM (iceoryx) data-plane impl —
    # uses real `iox_chunk_header_t`, `AllocationResult_*`,
    # `Iceoryx_LogLevel_*` types our stubs don't define. Drop entirely
    # under DDS_HAS_SHM=0; call sites guarded internally.
    list(FILTER _cdds_ddsi EXCLUDE REGEX "ddsi_shm_transport\\.c$")
    # ddsc/src/shm_monitor.c — SHM data-plane monitor; uses real
    # iceoryx storage / event / result types. Drop under DDS_HAS_SHM=0.
    list(FILTER _cdds_ddsc EXCLUDE REGEX "shm_monitor\\.c$")

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
    # IMPORTANT: Cyclone checks these with a mix of `#ifdef
    # DDS_HAS_*` (defined-check) AND `#if DDS_HAS_*` (value-check).
    # Per-symbol policy:
    #   - SECURITY / SHM / TYPE_DISCOVERY / TOPIC_DISCOVERY:
    #     UNDEFINED. `#ifdef`-gated; falls through to inline
    #     no-op / error-returning stubs. Avoids 80+ link-time
    #     undefined references from security_check_* /
    #     iox_pub_* / etc. (Phase 11W.4).
    #   - NETWORK_PARTITIONS: DEFINED (no value).
    #     `free_config_networkpartition_addresses` is compiled
    #     UNCONDITIONALLY in q_init.c, but its body references
    #     `struct ddsi_config_networkpartition_listelem` which
    #     is itself `#ifdef`-gated. Mismatch in upstream Cyclone;
    #     defining the macro keeps the struct visible.
    zephyr_compile_definitions(
        DDS_HAS_NETWORK_PARTITIONS
        DDS_HAS_TYPE_DISCOVERY
        DDS_HAS_TOPIC_DISCOVERY
    )

    # Phase 11W.4 — IN_MULTICAST macro provided via the
    # `zephyr_ipv4_compat.h` force-include header instead of
    # cmake `-D` (cmake refuses function-style macro defines).

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
    # Phase 172.K — generate the rmw_dds_common ParticipantEntitiesInfo
    # descriptor + register TU that src/graph.cpp (177.36 ros_discovery_info)
    # needs. The standalone CMakeLists does this via the TypeSupport helper;
    # the Zephyr direct-compile path GLOBs src/*.cpp (now including graph.cpp),
    # so it must run the same idlc codegen and add the generated header dir to
    # the include path — else graph.cpp fails on
    # `#include "rmw_dds_common_graph.h"`. The helper has no imported
    # CycloneDDS::ddsc target on embedded, so pre-set IDLC_EXECUTABLE (the host
    # idlc built by `just cyclonedds setup`).
    # Resolve a host idlc (priority): explicit IDLC_EXECUTABLE env override,
    # the `nros setup` SDK store ($NROS_HOME/sdk or ~/.nros/sdk →
    # cyclonedds/<ver>/bin), host PATH (e.g. a ROS 2 install), then the
    # legacy in-tree build dirs. The retired Phase 140 `build/install`
    # prefix is only a last-resort hint.
    if(DEFINED ENV{IDLC_EXECUTABLE} AND EXISTS "$ENV{IDLC_EXECUTABLE}")
        set(NROS_HOST_IDLC "$ENV{IDLC_EXECUTABLE}" CACHE FILEPATH "host Cyclone idlc")
    else()
        if(DEFINED ENV{NROS_HOME})
            file(GLOB _nros_idlc_store_hints "$ENV{NROS_HOME}/sdk/cyclonedds/*/bin")
        else()
            file(GLOB _nros_idlc_store_hints "$ENV{HOME}/.nros/sdk/cyclonedds/*/bin")
        endif()
        find_program(NROS_HOST_IDLC
            NAMES idlc
            HINTS
                ${_nros_idlc_store_hints}
                "${NROS_REPO_DIR}/build/cyclonedds/bin"
                "${NROS_REPO_DIR}/build/install/bin"
            DOC "host Cyclone idlc (graph-types codegen)")
    endif()
    if(NOT NROS_HOST_IDLC)
        message(FATAL_ERROR
            "host Cyclone idlc not found — install ROS 2 (idlc on PATH), run "
            "`nros setup <board> --rmw cyclonedds`, or set IDLC_EXECUTABLE.")
    endif()
    set(IDLC_EXECUTABLE "${NROS_HOST_IDLC}"
        CACHE FILEPATH "host Cyclone idlc (graph-types codegen)" FORCE)
    list(APPEND CMAKE_MODULE_PATH "${NROS_RMW_CDDS_DIR}/cmake")
    include(NrosRmwCycloneddsTypeSupport)
    set(_nros_graph_types_dir "${CMAKE_CURRENT_BINARY_DIR}/nros-graph-types")
    nros_rmw_cyclonedds_idlc_compile(_nros_graph_desc_srcs
        IDL_FILE   "${NROS_RMW_CDDS_DIR}/src/idl/rmw_dds_common_graph.idl"
        OUTPUT_DIR "${_nros_graph_types_dir}"
        TYPE_NAME  "rmw_dds_common::msg::dds_::ParticipantEntitiesInfo_")

    file(GLOB _nros_cdds_cpp ${NROS_RMW_CDDS_DIR}/src/*.cpp)
    zephyr_library_sources(${_nros_cdds_cpp} ${_nros_graph_desc_srcs})
    zephyr_include_directories(
        ${NROS_RMW_CDDS_DIR}/include
        ${NROS_RMW_CDDS_DIR}/src
        ${NROS_REPO_DIR}/packages/core/nros-rmw-cffi/include
        ${_nros_graph_types_dir}
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

    # ---- Phase 180.B — module exports for copy-out-clean examples ----------
    # Examples must never walk the repo tree (`../../../..`) to find the
    # Cyclone descriptor-codegen tooling. Export the host idlc, the
    # descriptor-gen scripts dir, and the descriptor cmake helper dir as
    # cache vars. An example then references only these names (no repo
    # path): it `list(APPEND CMAKE_MODULE_PATH "${NROS_CYCLONE_CMAKE_DIR}")`
    # and `include(NrosRmwCycloneddsTypeSupport)` by bare name. (A cache
    # re-export of CMAKE_MODULE_PATH itself does NOT work — Zephyr's
    # find_package sets a *normal* CMAKE_MODULE_PATH in the app scope that
    # shadows the INTERNAL cache value, so the append must happen in the
    # app scope.) NROS_REPO_DIR is the repo root here (set above).
    set(NROS_CYCLONE_IDLC "${NROS_HOST_IDLC}"
        CACHE FILEPATH "host Cyclone idlc" FORCE)
    set(NROS_CYCLONE_SCRIPTS_DIR "${NROS_REPO_DIR}/scripts/cyclonedds"
        CACHE PATH "cyclone descriptor-gen scripts")
    set(NROS_CYCLONE_CMAKE_DIR "${NROS_RMW_CDDS_DIR}/cmake"
        CACHE PATH "nros-rmw-cyclonedds cmake helper dir")
    list(APPEND CMAKE_MODULE_PATH "${NROS_CYCLONE_CMAKE_DIR}")

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
    # Phase 190.L — but NOT with a full libstdc++ (CONFIG_GLIBCXX_LIBCPP): the
    # dir on the global -I breaks libstdc++'s own headers there. Skip it; the
    # real C++ stdlib already provides everything. (See the cyclonedds branch.)
    if(NOT CONFIG_GLIBCXX_LIBCPP)
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
        zephyr_library_link_libraries(nros_c_cargo)
    endif()

    # Build nros-cpp static library via Cargo
    nros_cargo_build(
        PACKAGE nros-cpp
        FEATURES "${_nros_cpp_features}"
    )
    add_dependencies(app nros_cpp_cargo_build)

    # Link the built library
    zephyr_library_link_libraries(nros_cpp_cargo)

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
