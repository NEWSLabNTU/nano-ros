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
#   nros_freertos_build_kernel(PORT <enum-or-path>
#                              [HEAP <name>]
#                              [EXTRA_SOURCES <files…>]
#                              [EXTRA_INCLUDES <dirs…>])
#       Build the kernel with upstream FreeRTOS-Kernel's own CMake
#       (phase-349 W3): defines `freertos_config` and hands upstream
#       FREERTOS_PORT (its enum, e.g. GCC_ARM_CM3; a path under
#       portable/ such as GCC/ARM_CM3 is mapped through upstream's
#       table) and FREERTOS_HEAP (default heap_4). Upstream's target
#       is `freertos_kernel`. Optimised with -O2 -w (vendored code).
#
#   nros_freertos_config_define(<definition>…)
#       Add a definition every TU including FreeRTOSConfig.h sees
#       (the kernel, its port and their consumers), via the
#       `freertos_config` INTERFACE (issue 1197).
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
# nros_freertos_board_port(<board> <port> [OVERRIDABLE])
#
# phase-484 W2 — a board's FreeRTOS port is a BOARD FACT, so the board SETS
# `FREERTOS_PORT` instead of deferring to whatever the shell exported. The
# overlays used to write `if(NOT DEFINED FREERTOS_PORT AND NOT DEFINED
# ENV{FREERTOS_PORT})`, and before W2b `activate.sh` exported `GCC/ARM_CM3`
# repo-wide; a shell sourced before W2b still carries it, and on a Cortex-R52
# board that M-profile port fails hundreds of lines into the kernel build. So:
#
#   * `ENV{FREERTOS_PORT}` is never read. If it is set to something else, the
#     configure says it was ignored, so a stale shell is visible, not silent.
#   * A `-DFREERTOS_PORT` that disagrees is REFUSED on a board with exactly one
#     port, and taken as given with `OVERRIDABLE` — a board whose real hardware
#     uses a vendor port the in-tree default stands in for (S32Z270's NXP
#     `GCC/ARM_CR52_GIC`).
# ----------------------------------------------------------------------
function(nros_freertos_board_port board port)
    cmake_parse_arguments(_NFBP "OVERRIDABLE" "" "" ${ARGN})
    if(DEFINED ENV{FREERTOS_PORT} AND NOT "$ENV{FREERTOS_PORT}" STREQUAL "${port}")
        message(WARNING
            "${board}: ignoring the environment's FREERTOS_PORT='$ENV{FREERTOS_PORT}'. "
            "The port is a board fact ('${port}'); a value inherited from the shell "
            "is not a source (RFC-0103 D2). Pass -DFREERTOS_PORT to override where "
            "the board allows it, or unset the variable.")
    endif()
    set(_port "${port}")
    if(DEFINED FREERTOS_PORT AND NOT FREERTOS_PORT STREQUAL "" AND NOT FREERTOS_PORT STREQUAL "${port}")
        if(_NFBP_OVERRIDABLE)
            set(_port "${FREERTOS_PORT}")
        else()
            message(FATAL_ERROR
                "${board}: FREERTOS_PORT is '${FREERTOS_PORT}', but this board has exactly "
                "one port: '${port}'. Drop the -DFREERTOS_PORT, or pick a board whose "
                "port you meant.")
        endif()
    endif()
    set(FREERTOS_PORT "${_port}" CACHE STRING "FreeRTOS portable-layer subdir for ${board}" FORCE)
    set(FREERTOS_PORT "${_port}" PARENT_SCOPE)
endfunction()

# ----------------------------------------------------------------------
# _nros_freertos_port_table(<freertos_dir> <out_enums> <out_paths>)
#
# phase-349 W3 — upstream's port vocabulary, READ from upstream. Every port
# upstream builds has a row in `portable/CMakeLists.txt`'s header-directory
# table:
#
#   $<$<STREQUAL:${FREERTOS_PORT},GCC_ARM_CM3>:${CMAKE_CURRENT_LIST_DIR}/GCC/ARM_CM3>
#
# so the enum <-> `portable/` path pairing is derived here rather than
# restated. A hand table would be one more copy of a fact upstream owns, and
# the path is not the enum with `/` for `_`: `GCC/ARM_CRx_No_GIC` is
# `GCC_ARM_CRX_NOGIC` and `ThirdParty/GCC/Posix` is `GCC_POSIX`.
# ----------------------------------------------------------------------
function(_nros_freertos_port_table freertos_dir out_enums out_paths)
    set(_table "${freertos_dir}/portable/CMakeLists.txt")
    set(_enums "")
    set(_paths "")
    if(EXISTS "${_table}")
        # A row may wrap (`GCC_POSIX>:` then the directory on the next line),
        # so match across whitespace, and take the row's first directory.
        # The patterns match upstream's literal `${CMAKE_CURRENT_LIST_DIR}`
        # TEXT; the `DI[R]` spelling keeps them from reading as a use of that
        # variable inside a function (check-cmake-function-list-dir).
        file(READ "${_table}" _text)
        string(REGEX MATCHALL
            "STREQUAL:\\$\\{FREERTOS_PORT\\},[A-Za-z0-9_]+>:[ \t\r\n]*\\$\\{CMAKE_CURRENT_LIST_DI[R]\\}/[^> \t\r\n]+"
            _rows "${_text}")
        foreach(_row IN LISTS _rows)
            if(_row MATCHES
               "FREERTOS_PORT\\},([A-Za-z0-9_]+)>:[ \t\r\n]*\\$\\{CMAKE_CURRENT_LIST_DI[R]\\}/(.+)$")
                list(APPEND _enums "${CMAKE_MATCH_1}")
                list(APPEND _paths "${CMAKE_MATCH_2}")
            endif()
        endforeach()
    endif()
    set(${out_enums} "${_enums}" PARENT_SCOPE)
    set(${out_paths} "${_paths}" PARENT_SCOPE)
endfunction()

# ----------------------------------------------------------------------
# nros_freertos_config_define(<definition>…)
#
# A definition that changes what `FreeRTOSConfig.h` says (the heap size, a
# feature switch) must reach EVERY translation unit that includes it — issue
# 1197. With upstream's CMake the kernel is two targets: `freertos_kernel` and
# the port's OBJECT library `freertos_kernel_port`, which links only
# `freertos_kernel_include` -> `freertos_config`. A PUBLIC definition on
# `freertos_kernel` never reaches `port.c`, so the one home that reaches every
# TU, the kernel's consumers included, is the `freertos_config` INTERFACE.
# ----------------------------------------------------------------------
function(nros_freertos_config_define)
    if(NOT TARGET freertos_config)
        message(FATAL_ERROR
            "nros_freertos_config_define: no `freertos_config` target. It is "
            "created by nros_freertos_build_kernel(), or by a host project that "
            "builds FreeRTOS with upstream's CMake.")
    endif()
    target_compile_definitions(freertos_config INTERFACE ${ARGN})
endfunction()

# ----------------------------------------------------------------------
# nros_freertos_build_kernel
#
# phase-349 W3 — the kernel is built by UPSTREAM's `CMakeLists.txt`, not from a
# source list of ours. This function only supplies what upstream asks its
# consumer for: a `freertos_config` INTERFACE target (FreeRTOSConfig.h),
# `FREERTOS_PORT` in upstream's enum, and `FREERTOS_HEAP`. Upstream names the
# library `freertos_kernel`, which is what every consumer already links.
#
# PORT accepts upstream's enum (`GCC_ARM_CM3`) or a path under `portable/`
# (`GCC/ARM_CM3`, issue 0530); a path is mapped to its enum through upstream's
# own table. A path upstream does not list — a vendor port dropped into
# `portable/`, such as NXP's `GCC/ARM_CR52_GIC` — goes through upstream's
# `A_CUSTOM_PORT` hook, built from the `.c` and `.S` files in that directory.
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
    if(NOT EXISTS "${FREERTOS_DIR}/CMakeLists.txt")
        message(FATAL_ERROR
            "nros_freertos_build_kernel: ${FREERTOS_DIR} has no CMakeLists.txt. "
            "nano-ros builds the kernel with upstream FreeRTOS-Kernel's own CMake, "
            "which it has shipped since V10.5.0; point FREERTOS_DIR at such a tree.")
    endif()

    _nros_freertos_port_table("${FREERTOS_DIR}" _enums _paths)
    set(_enum "")
    set(_port_path "")
    if(_NFBK_PORT MATCHES "/")
        list(FIND _paths "${_NFBK_PORT}" _i)
        if(_i GREATER -1)
            list(GET _enums ${_i} _enum)
            set(_port_path "${_NFBK_PORT}")
        elseif(EXISTS "${FREERTOS_DIR}/portable/${_NFBK_PORT}/port.c")
            set(_enum A_CUSTOM_PORT)
            set(_port_path "${_NFBK_PORT}")
        endif()
    else()
        list(FIND _enums "${_NFBK_PORT}" _i)
        if(_i GREATER -1)
            set(_enum "${_NFBK_PORT}")
            list(GET _paths ${_i} _port_path)
        endif()
    endif()
    # Fail HERE, naming the variable and both spellings, rather than letting
    # upstream warn and the build report a missing `port.c` nobody asked for.
    if(NOT _enum)
        message(FATAL_ERROR
            "nros_freertos_build_kernel: FREERTOS_PORT '${_NFBK_PORT}' is neither "
            "a port upstream lists in ${FREERTOS_DIR}/portable/CMakeLists.txt nor "
            "a directory under portable/ with a port.c.\n"
            "  FREERTOS_PORT accepts either spelling:\n"
            "    upstream enum          : GCC_ARM_CM3\n"
            "    path under portable/   : GCC/ARM_CM3")
    endif()
    set(_port_dir "${FREERTOS_DIR}/portable/${_port_path}")
    if(NOT _NFBK_PORT STREQUAL _enum)
        message(STATUS "nano-ros: FreeRTOS port ${_NFBK_PORT} -> ${_enum}")
    endif()

    # The A/R-profile ports carry their context switch in assembly
    # (`portASM.S`); without ASM enabled, add_library() drops a `.S` source
    # SILENTLY and the link fails on FreeRTOS_IRQ_Handler four targets later.
    file(GLOB _port_asm "${_port_dir}/*.S")
    if(_port_asm)
        enable_language(ASM)
    endif()

    if(NOT TARGET freertos_config)
        add_library(freertos_config INTERFACE)
        target_include_directories(freertos_config SYSTEM INTERFACE
            "${FREERTOS_CONFIG_DIR}" ${_NFBK_EXTRA_INCLUDES})
    endif()

    if(_enum STREQUAL "A_CUSTOM_PORT")
        file(GLOB _port_c "${_port_dir}/*.c")
        add_library(freertos_kernel_port OBJECT ${_port_c} ${_port_asm})
        add_library(freertos_kernel_port_headers INTERFACE)
        target_include_directories(freertos_kernel_port_headers INTERFACE "${_port_dir}")
        target_link_libraries(freertos_kernel_port
            PUBLIC freertos_kernel_port_headers
            PRIVATE freertos_kernel_include)
    endif()

    # Upstream reads these as ordinary variables of the including scope.
    # `FREERTOS_PORT` shadows the board's CACHE entry (a path, for the
    # alias) with the enum for this one call.
    set(FREERTOS_PORT "${_enum}")
    string(REGEX REPLACE "^heap_" "" FREERTOS_HEAP "${_NFBK_HEAP}")
    add_subdirectory("${FREERTOS_DIR}" "${CMAKE_BINARY_DIR}/nros-freertos-kernel"
                     EXCLUDE_FROM_ALL)

    # Vendored code: warning-noisy under the tree's strict flags.
    target_compile_options(freertos_kernel PRIVATE -O2 -w)
    target_compile_options(freertos_kernel_port PRIVATE -O2 -w)
    if(_NFBK_EXTRA_SOURCES)
        target_sources(freertos_kernel PRIVATE ${_NFBK_EXTRA_SOURCES})
    endif()

    set(NROS_FREERTOS_INCLUDES
        "${FREERTOS_CONFIG_DIR}"
        "${FREERTOS_DIR}/include"
        "${_port_dir}"
        ${_NFBK_EXTRA_INCLUDES}
        PARENT_SCOPE)
    set(NROS_FREERTOS_PORT_DIR "${_port_dir}" PARENT_SCOPE)
endfunction()

# ----------------------------------------------------------------------
# _nros_lwip_filelist(<lwip_dir> <list_name> <out_var>)
#
# phase-349 W3 — read one `set(<list_name> …)` source list out of lwIP's
# `src/Filelists.cmake` as TEXT. That file is designed to be `include()`d, but
# including it is not read-only: it `configure_file`s `src/include/lwip/init.h`
# and `doc/doxygen/lwip.Doxyfile` INTO the lwIP source tree, which is a
# vendored, possibly shared store directory (and an `init.h` rewrite re-stales
# every TU including it). So the lists are parsed and the file is never run.
# ----------------------------------------------------------------------
function(_nros_lwip_filelist lwip_dir list_name out_var)
    set(_f "${lwip_dir}/src/Filelists.cmake")
    if(NOT EXISTS "${_f}")
        message(FATAL_ERROR
            "nros_freertos_build_lwip: ${_f} not found — LWIP_DIR must be an "
            "lwIP 2.1+ source tree (${lwip_dir}).")
    endif()
    file(READ "${_f}" _text)
    if(NOT _text MATCHES "set\\(${list_name}[ \t\r\n]+([^)]*)\\)")
        message(FATAL_ERROR
            "nros_freertos_build_lwip: no `set(${list_name} …)` in ${_f}.")
    endif()
    string(REGEX MATCHALL "\\$\\{LWIP_DIR\\}/[^ \t\r\n]+\\.c" _items "${CMAKE_MATCH_1}")
    set(_srcs "")
    foreach(_item IN LISTS _items)
        string(REPLACE "\${LWIP_DIR}" "${lwip_dir}" _item "${_item}")
        list(APPEND _srcs "${_item}")
    endforeach()
    if(NOT _srcs)
        message(FATAL_ERROR
            "nros_freertos_build_lwip: `${list_name}` in ${_f} lists no sources.")
    endif()
    set(${out_var} "${_srcs}" PARENT_SCOPE)
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

    # phase-349 W3 — the source list is upstream's, read from
    # `src/Filelists.cmake`: the IPv4 core, the sequential and socket APIs, and
    # Ethernet framing. What lwIP compiles is then what lwIP says it has, and a
    # file added upstream arrives with the next pin. Disabled features compile
    # to empty units (`LWIP_DHCP`, `LWIP_ALTCP` … in lwipopts.h decide).
    _nros_lwip_filelist("${LWIP_DIR}" lwipcore_SRCS _core)
    _nros_lwip_filelist("${LWIP_DIR}" lwipcore4_SRCS _core4)
    _nros_lwip_filelist("${LWIP_DIR}" lwipapi_SRCS _api)
    set(_lwip_sources
        ${_core} ${_core4} ${_api}
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
        # phase-372 (issue 0475's class) — a linker script reached through a
        # raw `-T` flag gets no rebuild edge, so an edited .ld leaves every
        # image linked against the OLD layout until something else forces a
        # relink (this is exactly how the .bss-LMA loader fix stayed
        # invisible on first rebuild). Register every -T argument (and its
        # shared INCLUDE, which lives next to it or on a -L path) as a link
        # dependency of the composed platform.
        foreach(_opt IN LISTS _NFCP_LINK_OPTIONS)
            if(_opt MATCHES "^-T(.+)$")
                set_property(TARGET freertos_platform APPEND PROPERTY
                    INTERFACE_LINK_DEPENDS "${CMAKE_MATCH_1}")
            endif()
        endforeach()
    endif()
endfunction()
