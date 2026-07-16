function(nros_zephyr_configure_rmw_cyclonedds)
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
zephyr_include_directories(${NROS_ZEPHYR_DIR}/cyclonedds-config)

# Force-include a Zephyr IPv4 compat header (NOT .S assembly — `-include`
# of a C header into asm breaks). Provides `struct ip_mreq` where Cyclone
# references it directly. Actual multicast joins go through the IGMP API
# in nros-platform-zephyr/src/net.c.
#
# Phase 180.A / phase-292 W2 (ASI wall #1) — scope the force-include to the
# nros library (where the cyclonedds TUs live) on EVERY Zephyr version, never
# globally:
#   * 4.x: a global zephyr_compile_options force-include also lands on
#     Zephyr's own heap_constants.c bootstrap TU, whose kernel.h pulls the
#     not-yet-generated <zephyr/heap_constants.h> (chicken-and-egg).
#   * 3.7: the global form poisons the GLOBAL interface with a genex whose
#     `$<OR:...,...>` carries a top-level COMMA — Zephyr 3.7's
#     llext-edk.cmake `$<JOIN:list,glue>` over the interface compile options
#     then fails to find its glue argument ("requires 2 comma separated
#     parameters, but got 1"), killing the GENERATE step for any consumer
#     app (ASI phase-3 W2 pin bump was the first to hit it; the llext-edk
#     custom command is evaluated even with CONFIG_LLEXT unset).
# Only the cyclonedds TUs need the header, and they all live in `nros`.
set(_nros_cdds_ipv4_compat
    "$<$<OR:$<COMPILE_LANGUAGE:C>,$<COMPILE_LANGUAGE:CXX>>:SHELL:-include ${NROS_ZEPHYR_DIR}/cyclonedds-config/zephyr_ipv4_compat.h>")
target_compile_options(nros PRIVATE ${_nros_cdds_ipv4_compat})

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
zephyr_include_directories(${NROS_ZEPHYR_DIR}/cxx-compat)

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
    ${NROS_ZEPHYR_DIR}/cyclonedds-zephyr/random_zephyr.c
    ${NROS_ZEPHYR_DIR}/cyclonedds-zephyr/process_zephyr.c
    # SHM stub fns — original ddsi_shm_transport.c dropped under
    # DDS_HAS_SHM=0 but downstream headers still publicly DDS_EXPORT
    # iox_sub_context_*() / shm_{lock,unlock}_iox_sub(). Stub bodies
    # satisfy the linker; never called at runtime.
    ${NROS_ZEPHYR_DIR}/cyclonedds-zephyr/shm_stubs.c
    # Phase 171.0.c — Zephyr's current minimal C++ runtime now
    # defines the nothrow new/delete overloads itself, but leaves
    # the `std::nothrow` tag object unresolved on AArch64 FVP.
    # Keep a tag-only TU instead of the old Phase 11W.3 operator
    # override, which now collides with Zephyr's cpp_new.cpp.
    ${NROS_ZEPHYR_DIR}/cyclonedds-zephyr/nothrow_tag.cpp
    # Phase 11W.4 — link-time stubs for the residual unreferenced
    # symbols (ddsi_vnet_init / ddsrt_getifaddrs / IN_MULTICAST
    # macro define). Most of the original 88 undef-references
    # disappeared once `DDS_HAS_*` was switched from `=0` to
    # leave-undefined; only these three needed explicit stubs.
    ${NROS_ZEPHYR_DIR}/cyclonedds-zephyr/link_stubs.c
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

    set(CMAKE_MODULE_PATH "${CMAKE_MODULE_PATH}" PARENT_SCOPE)
endfunction()
