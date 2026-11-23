# nros_socket_service_stack.cmake -- issue 2785.
# SPDX-License-Identifier: MIT OR Apache-2.0
#
# Zephyr's socket service thread (`net_socket_service`, CONFIG_NET_SOCKETS_SERVICE,
# which DHCPv4 and other net services select) polls every registered socket
# with `zsock_poll`, and Zephyr puts the whole poll array on the CALLER's
# stack: `struct k_poll_event poll_events[CONFIG_NET_SOCKETS_POLL_MAX]`
# (subsys/net/lib/sockets/sockets.c, `zsock_poll_internal`; ZVFS_POLL_MAX on
# 4.x). That array is 40 bytes per entry on a 64-bit target and 20 on a
# 32-bit one, and it scales with a knob nano-ros images raise for Cyclone
# (issue 1674), while CONFIG_NET_SOCKETS_SERVICE_STACK_SIZE stays where it
# was. No board here guards that stack in hardware, so the overflow is silent.
#
# MEASURED (issue 2785): a Zephyr Cyclone image on fvp_baser_aemv8r with
# NET_SOCKETS_POLL_MAX=256 and a 4096-byte service stack. The thread's saved
# stack pointer sat 12,816 bytes below its stack top, 8.7 KB under the stack's
# base, in .bss: 10,240 bytes of poll array plus 2,576 of everything else (the
# DHCP callback and the log record it prints run synchronously on this
# thread). Its frame records landed on nano-ros's log dispatch guard, which
# then read as held forever, so every nano-ros log record was queued and none
# printed: the image ran, and looked hung. Which variable the overflow hits
# moves with the link, so an image with one interface fewer booted.
#
# So the build refuses a stack that cannot hold the array plus that margin:
# 3,072 bytes of other frames on a 64-bit target (the measurement above,
# rounded up) and 1,024 on a 32-bit one. Not on native_sim
# (CONFIG_ARCH_POSIX): a Zephyr thread there runs on a host pthread's stack.

# The least CONFIG_NET_SOCKETS_SERVICE_STACK_SIZE the image's Kconfig values
# need, in bytes, or "" when the image has no socket service thread (or runs
# on native_sim). Reads the CONFIG_* variables Zephyr set from .config.
function(nros_socket_service_stack_floor out)
    set(${out} "" PARENT_SCOPE)
    if(NOT CONFIG_NET_SOCKETS_SERVICE OR CONFIG_ARCH_POSIX
       OR NOT DEFINED CONFIG_NET_SOCKETS_SERVICE_STACK_SIZE)
        return()
    endif()
    # The array is sized by ZVFS_POLL_MAX on 4.x (`zvfs_poll.c`) and by
    # NET_SOCKETS_POLL_MAX on 3.7 (`sockets.c`). Ask ZVFS first: 4.x still
    # defines NET_SOCKETS_POLL_MAX, deprecated with a default of 0, so asking
    # it first sized a 4.x poll array as empty (issue 2875).
    if(DEFINED CONFIG_ZVFS_POLL_MAX)
        set(_max "${CONFIG_ZVFS_POLL_MAX}")
    elseif(DEFINED CONFIG_NET_SOCKETS_POLL_MAX)
        set(_max "${CONFIG_NET_SOCKETS_POLL_MAX}")
    else()
        return()
    endif()
    if(CONFIG_64BIT)
        set(_entry 40)
        set(_margin 3072)
    else()
        set(_entry 20)
        set(_margin 1024)
    endif()
    math(EXPR _floor "${_max} * ${_entry} + ${_margin}")
    set(${out} "${_floor}" PARENT_SCOPE)
endfunction()

# Refuse the configure when the socket service stack is below its floor.
function(nros_check_socket_service_stack)
    nros_socket_service_stack_floor(_floor)
    if(_floor STREQUAL "")
        return()
    endif()
    if(CONFIG_NET_SOCKETS_SERVICE_STACK_SIZE LESS _floor)
        if(DEFINED CONFIG_ZVFS_POLL_MAX)
            set(_knob "CONFIG_ZVFS_POLL_MAX=${CONFIG_ZVFS_POLL_MAX}")
        else()
            set(_knob "CONFIG_NET_SOCKETS_POLL_MAX=${CONFIG_NET_SOCKETS_POLL_MAX}")
        endif()
        # Issue 2785.
        message(FATAL_ERROR
            "nros: CONFIG_NET_SOCKETS_SERVICE_STACK_SIZE="
            "${CONFIG_NET_SOCKETS_SERVICE_STACK_SIZE} cannot hold the socket service "
            "thread's poll: Zephyr puts one k_poll_event per poll entry on that "
            "thread's stack (${_knob}), and the thread also runs the services' "
            "callbacks. It needs at least ${_floor} bytes. Nothing guards that "
            "stack, so the overflow writes over .bss with no message. Set "
            "CONFIG_NET_SOCKETS_SERVICE_STACK_SIZE=${_floor} or more in the image's "
            "conf, or lower the poll maximum.")
    endif()
endfunction()
