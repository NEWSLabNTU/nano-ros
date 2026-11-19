# NanoRosThreadxPool.cmake — issue 2624: the ThreadX byte pool's defines on
# the cmake road.
#
# `threadx_hooks.c` sizes the byte pool (the port's whole allocator) from three
# defines; the cargo road's board build passes them through
# `nros_board_common::threadx_sources::define_byte_pool`, and this is the same
# answer for a board compiled by CMake:
#
#   NROS_THREADX_APP_STACK_BYTES  the app thread's stack, which the pool holds
#   NROS_THREADX_POOL_DDS         1 for a DDS backend: the larger measured
#                                 working set (the cargo road's rule: anything
#                                 but zenoh)
#   NROS_THREADX_BYTE_POOL_BYTES  only when stated: a `-DNROS_THREADX_BYTE_POOL_KB`,
#                                 else the environment or the image's
#                                 `[image.<id>.knobs]` row (`nros_image_env_value`).
#                                 Unstated, the C file derives it.
#
# A stated pool below the app stack plus the measured working set fails the
# C file's `_Static_assert`, which names the knob.

include_guard(GLOBAL)
include("${CMAKE_CURRENT_LIST_DIR}/NanoRosImageEnv.cmake")

# nros_threadx_byte_pool_defines(<out_var> <app_stack_bytes>)
function(nros_threadx_byte_pool_defines _out _app_stack)
    set(_defs "NROS_THREADX_APP_STACK_BYTES=${_app_stack}")
    string(TOLOWER "${NROS_RMW}" _rmw)
    if(NOT _rmw STREQUAL "" AND NOT _rmw STREQUAL "zenoh")
        list(APPEND _defs "NROS_THREADX_POOL_DDS=1")
    endif()
    if(DEFINED NROS_THREADX_BYTE_POOL_KB AND NOT "${NROS_THREADX_BYTE_POOL_KB}" STREQUAL "")
        set(_kb "${NROS_THREADX_BYTE_POOL_KB}")
    else()
        nros_image_env_value(_kb NROS_THREADX_BYTE_POOL_KB)
    endif()
    if(NOT "${_kb}" STREQUAL "")
        if(NOT _kb MATCHES "^[0-9]+$")
            message(FATAL_ERROR
                "nano-ros: NROS_THREADX_BYTE_POOL_KB='${_kb}': expected a decimal KiB "
                "count for the ThreadX byte pool (`[knobs.memory] heap_bytes`)")
        endif()
        math(EXPR _bytes "${_kb} * 1024")
        list(APPEND _defs "NROS_THREADX_BYTE_POOL_BYTES=${_bytes}")
    endif()
    set(${_out} "${_defs}" PARENT_SCOPE)
endfunction()
