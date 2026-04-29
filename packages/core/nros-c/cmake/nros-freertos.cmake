# nros-freertos.cmake
#
# Per-RTOS cmake module for FreeRTOS. Built on
# nros-rtos-helpers.cmake. Phase 91.E1b: layer-3 per-platform support
# files (freertos-support.cmake on MPS2-AN385, future ports on
# Cortex-M4 / Cortex-A / RISC-V) shrink to ~20-line orchestrators.
#
# Public functions:
#
#   nros_freertos_validate(REQUIRE <vars…>)
#       Validate the listed cmake variables (env-or-fatal-error).
#       Always requires FREERTOS_DIR and FREERTOS_CONFIG_DIR plus
#       whatever the caller passes in REQUIRE.
#
#   nros_freertos_build_kernel(PORT <subdir>
#                              [HEAP <name>]
#                              [EXTRA_SOURCES <files…>]
#                              [EXTRA_INCLUDES <dirs…>])
#       Build the freertos_kernel STATIC library. PORT is the suffix
#       under "${FREERTOS_DIR}/portable/" (e.g. "GCC/ARM_CM3"). HEAP
#       picks the heap implementation under
#       "${FREERTOS_DIR}/portable/MemMang/" (default: "heap_4").
#       Source list covers the standard kernel set: tasks, queue,
#       list, timers, event_groups, stream_buffer, port.c, heap_X.c.
#       Optimised with -O2 -w by default (FreeRTOS upstream is
#       warning-noisy under strict compile flags).
#
#   nros_freertos_build_lwip([CONTRIB_DIR <dir>]
#                            [EXTRA_SOURCES <files…>]
#                            [EXTRA_INCLUDES <dirs…>])
#       Build the lwip STATIC library from "${LWIP_DIR}/src/{core,
#       core/ipv4, api, netif}/*.c" plus "contrib/ports/freertos/
#       sys_arch.c". PUBLIC includes the FreeRTOS + lwip headers so
#       drivers and example sources find them transitively.
#
#   nros_freertos_build_netif(NAME <name>
#                             SOURCES <files…>
#                             INCLUDES <dirs…>)
#       Build a network-interface driver as STATIC, linked into the
#       platform target by nros_freertos_compose_platform. PUBLIC
#       includes propagate so other libs can reach the driver's
#       public header. Generic — works for LAN9118, ENET, any future
#       lwIP-compatible netif.
#
#   nros_freertos_compose_platform([COMPONENTS <libs…>]
#                                  [LINK_LIBS <libs…>]
#                                  [LINK_OPTIONS <opts…>])
#       Compose freertos_platform INTERFACE. Default COMPONENTS auto-
#       detects netif targets registered via build_netif then links
#       lwip + freertos_kernel. LINK_OPTIONS forwards INTERFACE
#       linker flags (typical: -T<linker_script>, -Wl,--gc-sections,
#       -nostartfiles, --specs=nosys.specs).
#
# Variables set by this module (parent scope of caller after
# nros_freertos_validate / build_kernel / build_lwip):
#
#   NROS_FREERTOS_INCLUDES       — kernel include set
#   NROS_FREERTOS_LWIP_INCLUDES  — lwip include set
#   NROS_FREERTOS_PORT_DIR       — "${FREERTOS_DIR}/portable/${PORT}"

if(DEFINED _NROS_FREERTOS_INCLUDED)
    return()
endif()
set(_NROS_FREERTOS_INCLUDED TRUE)

include("${CMAKE_CURRENT_LIST_DIR}/nros-rtos-helpers.cmake")

# Tracks netif libraries registered via nros_freertos_build_netif so
# compose_platform can auto-link them. Cached as a global property —
# reaches across function call frames without polluting variables.
define_property(GLOBAL PROPERTY NROS_FREERTOS_NETIFS
    BRIEF_DOCS "FreeRTOS netif driver targets"
    FULL_DOCS  "Set of STATIC libs created by nros_freertos_build_netif")

# ----------------------------------------------------------------------
# nros_freertos_validate
# ----------------------------------------------------------------------
function(nros_freertos_validate)
    cmake_parse_arguments(_NFV "" "" "REQUIRE" ${ARGN})
    nros_validate_vars(FREERTOS_DIR FREERTOS_CONFIG_DIR ${_NFV_REQUIRE})

    set(FREERTOS_DIR        "${FREERTOS_DIR}"        PARENT_SCOPE)
    set(FREERTOS_CONFIG_DIR "${FREERTOS_CONFIG_DIR}" PARENT_SCOPE)
    foreach(_v ${_NFV_REQUIRE})
        set(${_v} "${${_v}}" PARENT_SCOPE)
    endforeach()
endfunction()

# ----------------------------------------------------------------------
# nros_freertos_build_kernel
# ----------------------------------------------------------------------
function(nros_freertos_build_kernel)
    cmake_parse_arguments(_NFBK
        ""
        "PORT;HEAP"
        "EXTRA_SOURCES;EXTRA_INCLUDES"
        ${ARGN})

    if(NOT _NFBK_PORT)
        message(FATAL_ERROR "nros_freertos_build_kernel: PORT is required.")
    endif()
    if(NOT _NFBK_HEAP)
        set(_NFBK_HEAP heap_4)
    endif()

    set(_port_dir "${FREERTOS_DIR}/portable/${_NFBK_PORT}")
    set(_includes
        "${FREERTOS_CONFIG_DIR}"
        "${FREERTOS_DIR}/include"
        "${_port_dir}"
        ${_NFBK_EXTRA_INCLUDES})

    set(_sources
        "${FREERTOS_DIR}/tasks.c"
        "${FREERTOS_DIR}/queue.c"
        "${FREERTOS_DIR}/list.c"
        "${FREERTOS_DIR}/timers.c"
        "${FREERTOS_DIR}/event_groups.c"
        "${FREERTOS_DIR}/stream_buffer.c"
        "${_port_dir}/port.c"
        "${FREERTOS_DIR}/portable/MemMang/${_NFBK_HEAP}.c"
        ${_NFBK_EXTRA_SOURCES})

    add_library(freertos_kernel STATIC ${_sources})
    target_include_directories(freertos_kernel PUBLIC ${_includes})
    target_compile_options(freertos_kernel PRIVATE -O2 -w)

    set(NROS_FREERTOS_INCLUDES "${_includes}" PARENT_SCOPE)
    set(NROS_FREERTOS_PORT_DIR "${_port_dir}" PARENT_SCOPE)
endfunction()

# ----------------------------------------------------------------------
# nros_freertos_build_lwip
# ----------------------------------------------------------------------
function(nros_freertos_build_lwip)
    cmake_parse_arguments(_NFL
        ""
        "CONTRIB_DIR"
        "EXTRA_SOURCES;EXTRA_INCLUDES"
        ${ARGN})

    nros_validate_vars(LWIP_DIR)
    set(LWIP_DIR "${LWIP_DIR}" PARENT_SCOPE)

    if(NOT _NFL_CONTRIB_DIR)
        set(_NFL_CONTRIB_DIR "${LWIP_DIR}/contrib/ports/freertos")
    endif()

    set(_lwip_includes
        "${LWIP_DIR}/src/include"
        "${_NFL_CONTRIB_DIR}/include"
        ${_NFL_EXTRA_INCLUDES})

    set(_lwip_sources
        # Core
        "${LWIP_DIR}/src/core/init.c"
        "${LWIP_DIR}/src/core/def.c"
        "${LWIP_DIR}/src/core/dns.c"
        "${LWIP_DIR}/src/core/inet_chksum.c"
        "${LWIP_DIR}/src/core/ip.c"
        "${LWIP_DIR}/src/core/mem.c"
        "${LWIP_DIR}/src/core/memp.c"
        "${LWIP_DIR}/src/core/netif.c"
        "${LWIP_DIR}/src/core/pbuf.c"
        "${LWIP_DIR}/src/core/raw.c"
        "${LWIP_DIR}/src/core/stats.c"
        "${LWIP_DIR}/src/core/sys.c"
        "${LWIP_DIR}/src/core/tcp.c"
        "${LWIP_DIR}/src/core/tcp_in.c"
        "${LWIP_DIR}/src/core/tcp_out.c"
        "${LWIP_DIR}/src/core/timeouts.c"
        "${LWIP_DIR}/src/core/udp.c"
        # IPv4
        "${LWIP_DIR}/src/core/ipv4/etharp.c"
        "${LWIP_DIR}/src/core/ipv4/icmp.c"
        # Phase 97.1.kconfig.freertos — `LWIP_IGMP=1` is set in
        # lwipopts.h to support RTPS SPDP multicast. Without igmp.c
        # here, `igmp_init` / `igmp_joingroup` / `igmp_leavegroup`
        # / `lwip_netconn_do_join_leave_group` end up undefined at
        # link time in C / C++ FreeRTOS examples sharing this lwIP
        # build.
        "${LWIP_DIR}/src/core/ipv4/igmp.c"
        "${LWIP_DIR}/src/core/ipv4/ip4.c"
        "${LWIP_DIR}/src/core/ipv4/ip4_addr.c"
        "${LWIP_DIR}/src/core/ipv4/ip4_frag.c"
        # API (sockets)
        "${LWIP_DIR}/src/api/api_lib.c"
        "${LWIP_DIR}/src/api/api_msg.c"
        "${LWIP_DIR}/src/api/err.c"
        "${LWIP_DIR}/src/api/if_api.c"
        "${LWIP_DIR}/src/api/netbuf.c"
        "${LWIP_DIR}/src/api/netdb.c"
        "${LWIP_DIR}/src/api/netifapi.c"
        "${LWIP_DIR}/src/api/sockets.c"
        "${LWIP_DIR}/src/api/tcpip.c"
        # Netif
        "${LWIP_DIR}/src/netif/ethernet.c"
        # FreeRTOS sys_arch
        "${_NFL_CONTRIB_DIR}/sys_arch.c"
        ${_NFL_EXTRA_SOURCES})

    add_library(lwip STATIC ${_lwip_sources})
    target_include_directories(lwip PUBLIC
        ${NROS_FREERTOS_INCLUDES} ${_lwip_includes})
    target_compile_options(lwip PRIVATE -O2 -w)

    set(NROS_FREERTOS_LWIP_INCLUDES "${_lwip_includes}" PARENT_SCOPE)
endfunction()

# ----------------------------------------------------------------------
# nros_freertos_build_netif
# ----------------------------------------------------------------------
function(nros_freertos_build_netif)
    cmake_parse_arguments(_NFN
        ""
        "NAME"
        "SOURCES;INCLUDES"
        ${ARGN})

    if(NOT _NFN_NAME OR NOT _NFN_SOURCES)
        message(FATAL_ERROR
            "nros_freertos_build_netif: NAME and SOURCES are required.")
    endif()

    add_library(${_NFN_NAME} STATIC ${_NFN_SOURCES})
    target_include_directories(${_NFN_NAME} PUBLIC
        ${_NFN_INCLUDES}
        ${NROS_FREERTOS_INCLUDES}
        ${NROS_FREERTOS_LWIP_INCLUDES})
    target_compile_options(${_NFN_NAME} PRIVATE -O2 -w)

    set_property(GLOBAL APPEND PROPERTY NROS_FREERTOS_NETIFS ${_NFN_NAME})
endfunction()

# ----------------------------------------------------------------------
# nros_freertos_compose_platform
# ----------------------------------------------------------------------
function(nros_freertos_compose_platform)
    cmake_parse_arguments(_NFCP
        ""
        ""
        "COMPONENTS;LINK_LIBS;LINK_OPTIONS"
        ${ARGN})

    if(NOT _NFCP_COMPONENTS)
        get_property(_netifs GLOBAL PROPERTY NROS_FREERTOS_NETIFS)
        set(_NFCP_COMPONENTS ${_netifs})
        if(TARGET lwip)
            list(APPEND _NFCP_COMPONENTS lwip)
        endif()
        list(APPEND _NFCP_COMPONENTS freertos_kernel)
    endif()

    nros_compose_platform_target(freertos_platform
        COMPONENTS ${_NFCP_COMPONENTS}
        LINK_LIBS  ${_NFCP_LINK_LIBS})

    if(_NFCP_LINK_OPTIONS)
        target_link_options(freertos_platform INTERFACE ${_NFCP_LINK_OPTIONS})
    endif()
endfunction()
