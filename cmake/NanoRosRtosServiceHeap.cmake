# NanoRosRtosServiceHeap.cmake -- issue 2812: the per-node service term of the
# FreeRTOS heap and the ThreadX byte pool, on the cmake road.
#
# Both ports size their allocator from a working set MEASURED on one-node
# images (`FreeRTOSConfig.h`'s `NROS_FREERTOS_*_HEAP_KB`, `threadx_hooks.c`'s
# `NROS_THREADX_*_WORKING_SET_BYTES`). Every further node serving the parameter
# services, and every further lifecycle state machine, puts its servers'
# buffer pairs on that allocator, so the image's own counts add a term:
# `cmake/NanoRosServiceHeap.cmake`'s formula (shared with Zephyr's heap floor
# and with the cargo road's `nros_sizing_descriptor::service_heap`), with each
# port's measured remainder below.
#
# The counts are the entity inventory's NROS_ENTITY_PARAM_SERVICE_NODES and
# NROS_ENTITY_LIFECYCLE_MACHINES, which `nano_ros_entry` has just written to
# the image's fragment. One node and one machine (or no fragment) add 0, and
# then nothing is defined, so such an image compiles exactly as before.

include_guard(GLOBAL)
include("${CMAKE_CURRENT_LIST_DIR}/NanoRosServiceHeap.cmake")

# What one more node (machine) costs each port beyond its buffer pairs,
# MEASURED. The cargo road's twins are `PARAM_NODE_REMAINDER_BYTES` /
# `LIFECYCLE_MACHINE_REMAINDER_BYTES` in nros-board-common's
# `freertos_config.rs` and `threadx_pool.rs`, whose tests read these lines.
# CACHE INTERNAL, so they survive this file being included from inside a
# function frame (the `_NROS_ENTRY_DIR` hazard).
set(NROS_FREERTOS_PARAM_NODE_REMAINDER 0 CACHE INTERNAL "issue 2812: measured")
set(NROS_FREERTOS_LIFECYCLE_MACHINE_REMAINDER 0 CACHE INTERNAL "issue 2812: measured")
set(NROS_THREADX_PARAM_NODE_REMAINDER 0 CACHE INTERNAL "issue 2812: measured")
set(NROS_THREADX_LIFECYCLE_MACHINE_REMAINDER 0 CACHE INTERNAL "issue 2812: measured")

# nros_rtos_service_heap_extra(<fragment> <port> <out_extra> <out_comp>)
#
# <port> is FREERTOS or THREADX. Reads the counts from the entity-inventory
# <fragment> (in this function's scope, so nothing leaks) and sets <out_extra>
# to the bytes the image needs beyond its one-node default.
function(nros_rtos_service_heap_extra fragment port out_extra out_comp)
    set(${out_extra} 0 PARENT_SCOPE)
    set(${out_comp} "" PARENT_SCOPE)
    if("${fragment}" STREQUAL "" OR NOT EXISTS "${fragment}")
        return()
    endif()
    unset(NROS_ENTITY_PARAM_SERVICE_NODES)
    unset(NROS_ENTITY_LIFECYCLE_MACHINES)
    include("${fragment}")
    _nros_service_heap_count(NROS_ENTITY_PARAM_SERVICE_NODES _n)
    _nros_service_heap_count(NROS_ENTITY_LIFECYCLE_MACHINES _m)
    # The buffer is nros-node's default: NROS_EXECUTOR_PARAM_SERVICE_BUFFER_BYTES has one
    # reader (nros-node's build script), and every in-tree image runs the
    # default. A stated smaller buffer over-provisions by the difference.
    _nros_service_heap_extra("${_n}" "${_m}" ""
        "${NROS_${port}_PARAM_NODE_REMAINDER}"
        "${NROS_${port}_LIFECYCLE_MACHINE_REMAINDER}"
        _extra _comp)
    set(${out_extra} "${_extra}" PARENT_SCOPE)
    set(${out_comp} "${_comp}" PARENT_SCOPE)
endfunction()

# nros_rtos_service_heap_apply(<entry> <fragment>)
#
# Hands the extra to whichever port this configure builds:
#   - FreeRTOS: `NROS_FREERTOS_SERVICE_HEAP_EXTRA_KB` on `freertos_config`
#     (every TU that reads `configTOTAL_HEAP_SIZE` must agree, issue 1197),
#     rounded UP to whole KiB;
#   - ThreadX: `NROS_THREADX_SERVICE_HEAP_EXTRA_BYTES` on `threadx_glue`, the
#     library that compiles `threadx_hooks.c`.
# One kernel has one heap, so two entries in one configure that need different
# extras are refused, as the sched-task stack move is.
function(nros_rtos_service_heap_apply entry fragment)
    if(TARGET freertos_config)
        set(_port FREERTOS)
    elseif(TARGET threadx_glue)
        set(_port THREADX)
    else()
        return()
    endif()
    nros_rtos_service_heap_extra("${fragment}" ${_port} _extra _comp)
    if(_extra EQUAL 0)
        return()
    endif()
    get_property(_prev GLOBAL PROPERTY _NROS_RTOS_SERVICE_HEAP_EXTRA)
    if(NOT "${_prev}" STREQUAL "")
        if(NOT _prev EQUAL _extra)
            message(FATAL_ERROR
                "nano_ros_entry(${entry}): this configure already sized the shared "
                "${_port} heap for an entry whose further parameter-serving nodes and "
                "lifecycle machines add ${_prev} bytes; this one adds ${_extra}. One "
                "kernel has one heap, so these entries need two build directories.")
        endif()
        return()
    endif()
    set_property(GLOBAL PROPERTY _NROS_RTOS_SERVICE_HEAP_EXTRA "${_extra}")
    message(STATUS "nano_ros_entry(${entry}): ${_port} heap +${_extra} bytes${_comp}")
    if(_port STREQUAL "FREERTOS")
        math(EXPR _kb "(${_extra} + 1023) / 1024")
        nros_freertos_config_define("NROS_FREERTOS_SERVICE_HEAP_EXTRA_KB=${_kb}")
    else()
        target_compile_definitions(threadx_glue PRIVATE
            "NROS_THREADX_SERVICE_HEAP_EXTRA_BYTES=${_extra}u")
    endif()
endfunction()
