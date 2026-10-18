# integrations/freertos/NanoRosFreertos.cmake
#
# phase-349 W4 (RFC-0072) — the FreeRTOS integration shell: nano-ros as a GUEST
# in a CMake project that already builds FreeRTOS.
#
# The host project owns the kernel. Upstream FreeRTOS-Kernel's own CMake (since
# V10.5.0) produces exactly the targets this shell consumes:
#
#   freertos_config  INTERFACE  — yours: carries FreeRTOSConfig.h. Upstream's
#                                 CMakeLists.txt requires you to define it.
#   freertos_kernel  library    — upstream's, from add_subdirectory(<kernel>)
#                                 with FREERTOS_PORT (its enum) and
#                                 FREERTOS_HEAP set.
#   lwip             library    — your TCP/IP stack (any target name; pass it
#                                 as NETSTACK).
#
# The shell checks those, then composes `freertos_platform`, the umbrella every
# nano-ros image links. Composed BEFORE add_subdirectory(<nano-ros>), it puts
# `cmake/platform/nano-ros-freertos.cmake` into board-less mode, so a host
# project needs no file in the nano-ros tree. nano-ros's own FreeRTOS boards
# call the same function after building those targets with upstream's CMake
# (phase-349 W3), so the boards are this shell's first users and both go
# through one consumption path.
#
# Usage, in the host project:
#
#   set(FREERTOS_PORT GCC_ARM_CM3)          # upstream's enum
#   set(FREERTOS_HEAP 4)
#   add_library(freertos_config INTERFACE)
#   target_include_directories(freertos_config SYSTEM INTERFACE config/)
#   add_subdirectory(third_party/FreeRTOS-Kernel)
#   add_library(lwip STATIC ...)            # your stack + its lwipopts.h
#
#   include(<nano-ros>/integrations/freertos/NanoRosFreertos.cmake)
#   nros_freertos_integrate(
#       NETSTACK     lwip
#       NETIFS       my_eth_netif
#       LINK_OPTIONS "-T${CMAKE_SOURCE_DIR}/board.ld" -nostartfiles)
#
#   add_subdirectory(<nano-ros> nano_ros)
#
# Arguments (all optional):
#
#   KERNEL <target>        default freertos_kernel
#   CONFIG <target>        default freertos_config
#   NETSTACK <target>|NONE default lwip. NONE states that the host's kernel
#                          owns the network stack (the FreeRTOS POSIX
#                          simulator, where tasks are pthreads and sockets are
#                          the host's).
#   NETIFS <targets…>      network-interface drivers to link
#   LINK_LIBS <libs…>      further libraries on the umbrella
#   LINK_OPTIONS <opts…>   link flags on the umbrella (-T<script>, …)
#
# What fails, and how (RFC-0072 §7, "What fails, and how") — at configure,
# naming the thing:
#
#   * a target the shell needs and the project did not define;
#   * a capability the platform requires and the shell was not given: the
#     FreeRTOS platform descriptor declares `ip_stack = true`, so NETSTACK is
#     required unless stated NONE;
#   * an include directory one of those targets exports that does not exist —
#     an uninitialised submodule or a mistyped path, which otherwise surfaces
#     as a missing header in the first TU that reaches it;
#   * a CONFIG target whose include directories hold no FreeRTOSConfig.h.

if(DEFINED _NROS_FREERTOS_SHELL_INCLUDED)
    return()
endif()
set(_NROS_FREERTOS_SHELL_INCLUDED TRUE)

# CACHE INTERNAL: a board overlay include()s this file, and an include() inside
# a function drops a plain variable when the frame pops.
get_filename_component(_NROS_FREERTOS_SHELL_ROOT "${CMAKE_CURRENT_LIST_DIR}/../.." ABSOLUTE)
set(_NROS_FREERTOS_SHELL_ROOT "${_NROS_FREERTOS_SHELL_ROOT}" CACHE INTERNAL
    "nano-ros root, as integrations/freertos found it")
set(_NROS_FREERTOS_SHELL_PLATFORM_TOML
    "${_NROS_FREERTOS_SHELL_ROOT}/packages/platform/nros-platform-freertos/nros-platform.toml"
    CACHE INTERNAL "the FreeRTOS platform descriptor the shell reads capabilities from")

include("${_NROS_FREERTOS_SHELL_ROOT}/packages/api/nros-c/cmake/nros-freertos.cmake")

# ----------------------------------------------------------------------
# _nros_freertos_shell_require_target(<role> <target> <how…>)
# ----------------------------------------------------------------------
function(_nros_freertos_shell_require_target role target)
    if(NOT TARGET ${target})
        string(CONCAT _how ${ARGN})
        message(FATAL_ERROR
            "integrations/freertos: no `${target}` target (${role}). ${_how}")
    endif()
endfunction()

# ----------------------------------------------------------------------
# _nros_freertos_shell_plain_entries(<list> <out_var>)
#
# The entries of a usage-requirement list that sit outside every generator
# expression. A genex is not one list element: upstream writes
#
#   $<$<STREQUAL:${FREERTOS_PORT},GCC_POSIX>:
#       ${CMAKE_CURRENT_LIST_DIR}/ThirdParty/GCC/Posix
#       ${CMAKE_CURRENT_LIST_DIR}/ThirdParty/GCC/Posix/utils>
#
# which reaches the property as three elements, the last ending in `>`. So the
# nesting depth is carried across elements, and an element is plain only when
# it starts and ends at depth 0 and opens no genex of its own. (Measured: a
# per-element `$<` test reported eleven of upstream's other ports' directories
# as missing on the first in-tree configure.)
# ----------------------------------------------------------------------
function(_nros_freertos_shell_plain_entries list out_var)
    set(_plain "")
    set(_depth 0)
    foreach(_e IN LISTS list)
        string(REGEX MATCHALL "\\$<" _opens "${_e}")
        string(REGEX MATCHALL ">" _closes "${_e}")
        list(LENGTH _opens _o)
        list(LENGTH _closes _c)
        if(_depth EQUAL 0 AND _o EQUAL 0 AND NOT _e STREQUAL "")
            list(APPEND _plain "${_e}")
        endif()
        math(EXPR _depth "${_depth} + ${_o} - ${_c}")
        if(_depth LESS 0)
            set(_depth 0)
        endif()
    endforeach()
    set(${out_var} "${_plain}" PARENT_SCOPE)
endfunction()

# ----------------------------------------------------------------------
# _nros_freertos_shell_include_dirs(<target> <out_var>)
#
# The include directories `target` hands its consumers: its own
# INTERFACE_INCLUDE_DIRECTORIES and those of every target it links on its
# interface, transitively. Upstream's `freertos_kernel` carries none itself —
# they sit on `freertos_kernel_include` and `freertos_config` behind it.
# Generator expressions are skipped: they are evaluated per consumer, and
# upstream's port-header row is one (`$<$<STREQUAL:${FREERTOS_PORT},…>:…>`).
# ----------------------------------------------------------------------
function(_nros_freertos_shell_include_dirs target out_var)
    set(_seen "")
    set(_queue "${target}")
    set(_dirs "")
    while(_queue)
        list(POP_FRONT _queue _t)
        if(NOT TARGET ${_t} OR _t IN_LIST _seen)
            continue()
        endif()
        list(APPEND _seen ${_t})
        get_target_property(_real ${_t} ALIASED_TARGET)
        if(_real)
            list(APPEND _queue ${_real})
            continue()
        endif()
        get_target_property(_inc ${_t} INTERFACE_INCLUDE_DIRECTORIES)
        if(_inc)
            _nros_freertos_shell_plain_entries("${_inc}" _inc)
            foreach(_d IN LISTS _inc)
                list(APPEND _dirs "${_t}=${_d}")
            endforeach()
        endif()
        get_target_property(_libs ${_t} INTERFACE_LINK_LIBRARIES)
        if(_libs)
            # `$<LINK_ONLY:x>` is how a STATIC library records a PRIVATE
            # dependency; it reaches no consumer's include path.
            _nros_freertos_shell_plain_entries("${_libs}" _libs)
            list(APPEND _queue ${_libs})
        endif()
    endwhile()
    set(${out_var} "${_dirs}" PARENT_SCOPE)
endfunction()

# ----------------------------------------------------------------------
# _nros_freertos_shell_platform_requires(<capability> <out_var>)
#
# TRUE when the FreeRTOS platform descriptor declares `<capability> = true`
# under `[capabilities]`. Read, not restated: the descriptor is the one place
# the platform's software-stack facts live (phase-400 W3 / RFC-0086).
# ----------------------------------------------------------------------
function(_nros_freertos_shell_platform_requires capability out_var)
    set(_req FALSE)
    if(EXISTS "${_NROS_FREERTOS_SHELL_PLATFORM_TOML}")
        file(STRINGS "${_NROS_FREERTOS_SHELL_PLATFORM_TOML}" _lines)
        set(_in FALSE)
        foreach(_ln IN LISTS _lines)
            if(_ln MATCHES "^[ \t]*\\[")
                if(_ln MATCHES "^[ \t]*\\[capabilities\\]")
                    set(_in TRUE)
                else()
                    set(_in FALSE)
                endif()
            elseif(_in AND _ln MATCHES "^[ \t]*${capability}[ \t]*=[ \t]*true")
                set(_req TRUE)
            endif()
        endforeach()
    endif()
    set(${out_var} ${_req} PARENT_SCOPE)
endfunction()

# ----------------------------------------------------------------------
# nros_freertos_integrate([KERNEL <t>] [CONFIG <t>] [NETSTACK <t>|NONE]
#                         [NETIFS <t…>] [LINK_LIBS <l…>] [LINK_OPTIONS <o…>])
# ----------------------------------------------------------------------
function(nros_freertos_integrate)
    cmake_parse_arguments(_NFI "" "KERNEL;CONFIG;NETSTACK"
        "NETIFS;LINK_LIBS;LINK_OPTIONS" ${ARGN})
    if(_NFI_UNPARSED_ARGUMENTS)
        message(FATAL_ERROR
            "nros_freertos_integrate: unexpected arguments: ${_NFI_UNPARSED_ARGUMENTS}")
    endif()
    if(TARGET freertos_platform)
        message(FATAL_ERROR
            "nros_freertos_integrate: `freertos_platform` already exists. The shell "
            "composes it; a project that composed its own does not call the shell.")
    endif()
    if(NOT _NFI_KERNEL)
        set(_NFI_KERNEL freertos_kernel)
    endif()
    if(NOT _NFI_CONFIG)
        set(_NFI_CONFIG freertos_config)
    endif()
    if(NOT DEFINED _NFI_NETSTACK)
        set(_NFI_NETSTACK lwip)
    endif()

    _nros_freertos_shell_require_target("the kernel" ${_NFI_KERNEL}
        "Upstream FreeRTOS-Kernel's add_subdirectory() defines `freertos_kernel` "
        "(pass KERNEL <target> for another name).")
    _nros_freertos_shell_require_target("FreeRTOSConfig.h" ${_NFI_CONFIG}
        "Define an INTERFACE target whose include directories hold "
        "FreeRTOSConfig.h; upstream's CMake requires the same target "
        "(pass CONFIG <target> for another name).")

    # A required capability nothing provides (RFC-0072 §7).
    string(TOUPPER "${_NFI_NETSTACK}" _netstack_uc)
    set(_netstack "")
    if(_netstack_uc STREQUAL "NONE")
        set(NROS_FREERTOS_BOARD_HAS_LWIP FALSE PARENT_SCOPE)
    else()
        if(NOT TARGET ${_NFI_NETSTACK})
            _nros_freertos_shell_platform_requires(ip_stack _need_ip)
            if(_need_ip)
                message(FATAL_ERROR
                    "integrations/freertos: capability `ip_stack` is not provided.\n"
                    "  required by : platform `freertos` "
                    "(${_NROS_FREERTOS_SHELL_PLATFORM_TOML}, [capabilities] ip_stack = true)\n"
                    "  provider    : NETSTACK `${_NFI_NETSTACK}`, which is not a target\n"
                    "Pass NETSTACK <your TCP/IP stack target> (lwIP), or NETSTACK NONE "
                    "when the host's kernel owns the network stack (the FreeRTOS POSIX "
                    "simulator).")
            endif()
            message(FATAL_ERROR
                "integrations/freertos: NETSTACK `${_NFI_NETSTACK}` is not a target.")
        endif()
        set(_netstack ${_NFI_NETSTACK})
        set(NROS_FREERTOS_BOARD_HAS_LWIP TRUE PARENT_SCOPE)
    endif()
    foreach(_n IN LISTS _NFI_NETIFS)
        _nros_freertos_shell_require_target("a NETIFS entry" ${_n}
            "Every NETIFS entry must be a target the project defined.")
    endforeach()

    # An include directory that does not exist (RFC-0072 §7).
    set(_all "")
    foreach(_t ${_NFI_CONFIG} ${_NFI_KERNEL} ${_netstack} ${_NFI_NETIFS})
        _nros_freertos_shell_include_dirs(${_t} _d)
        list(APPEND _all ${_d})
    endforeach()
    list(REMOVE_DUPLICATES _all)
    set(_missing "")
    foreach(_pair IN LISTS _all)
        string(FIND "${_pair}" "=" _eq)
        string(SUBSTRING "${_pair}" 0 ${_eq} _owner)
        math(EXPR _eq "${_eq} + 1")
        string(SUBSTRING "${_pair}" ${_eq} -1 _dir)
        if(NOT IS_DIRECTORY "${_dir}")
            string(APPEND _missing "\n  ${_dir}  (exported by `${_owner}`)")
        endif()
    endforeach()
    if(_missing)
        message(FATAL_ERROR
            "integrations/freertos: include directories that do not exist:${_missing}\n"
            "An uninitialised submodule or a mistyped path; fix it here rather than "
            "meet it as a missing header in the first file that includes one.")
    endif()

    _nros_freertos_shell_include_dirs(${_NFI_CONFIG} _cfg)
    set(_found FALSE)
    foreach(_pair IN LISTS _cfg)
        string(REGEX REPLACE "^[^=]*=" "" _dir "${_pair}")
        if(EXISTS "${_dir}/FreeRTOSConfig.h")
            set(_found TRUE)
        endif()
    endforeach()
    if(NOT _found)
        message(FATAL_ERROR
            "integrations/freertos: no FreeRTOSConfig.h in the include directories of "
            "`${_NFI_CONFIG}`. Every kernel TU and every nano-ros TU that includes "
            "FreeRTOS.h needs it.")
    endif()

    nros_freertos_compose_platform(
        COMPONENTS   ${_NFI_NETIFS} ${_netstack} ${_NFI_KERNEL}
        LINK_LIBS    ${_NFI_LINK_LIBS}
        LINK_OPTIONS ${_NFI_LINK_OPTIONS})
endfunction()
