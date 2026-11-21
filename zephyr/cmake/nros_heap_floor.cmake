# nros_heap_floor.cmake -- the per-node floor of NROS_MEMORY_HEAP_BYTES (issue
# 2780, fix (b) of its ruling).
#
# The runtime's own service families allocate their servers from the platform
# heap, and they are PER NODE:
#
#   - every node carrying the PARAMETER services runs six servers;
#   - every LIFECYCLE state machine runs five.
#
# Each server owns a request/reply pair of PARAM_SERVICE_BUFFER_SIZE bytes
# (`LIFECYCLE_SERVICE_BUFFER_SIZE` is the same constant, nros-node
# lifecycle_services.rs). Kconfig's flat arms were each priced on ONE node:
# 112 KiB for a C/C++ image with param_services, 192 KiB for a Rust one and for
# any lifecycle image. So a four-node image ran its arm dry at boot:
# `HEAP EXHAUSTED`, from `_z_write_filter_create`, the last allocation that did
# not fit.
#
# The floor is therefore DERIVED from what the image declares:
#
#     floor = BASE + extra
#
# where `extra` is cmake/NanoRosServiceHeap.cmake's `_nros_service_heap_extra`,
# the per-node arithmetic every port's heap derivation shares, fed this port's
# measured remainders.
#
# BASE is the arm the image had (one node, one machine), so an image with one of
# each derives exactly what it had before this file existed. The counts are the
# entity inventory's NROS_ENTITY_PARAM_SERVICE_NODES and
# NROS_ENTITY_LIFECYCLE_MACHINES, the same `InfraServices` counts the queryable
# pool is sized from. The floor reaches the knob through the existing derivable
# ladder: the environment and a stated value (Kconfig, or the image's
# `[image.<id>.knobs.memory] heap_bytes`) still win, and a stated value below the
# floor warns.
#
# The buffer is the STATED one when a rung states it, else nros-node's 4096
# default. When the contract's parameters derive a smaller one (in nros-node's
# build script, after this configure), the floor over-provisions by the
# difference, never under.

include_guard(GLOBAL)
# Constants CACHE INTERNAL for the reason NanoRosServiceHeap.cmake gives.
include("${CMAKE_CURRENT_LIST_DIR}/../../cmake/NanoRosServiceHeap.cmake")

# The arms, each the ONE-node case. 112 KiB was MEASURED on zephyr_cpp_params
# (one node, 46,848-byte peak), and holds for Rust too (issue 2772: the
# split Rust params image peaks at 45,840); 192 KiB on the lifecycle images
# (one node, one machine; see the knob's help).
set(NROS_ZEPHYR_HEAP_ONE_PARAM_NODE 114688 CACHE INTERNAL "issue 2780 heap floor")
set(NROS_ZEPHYR_HEAP_ONE_LIFECYCLE_MACHINE 196608 CACHE INTERNAL "issue 2780 heap floor")
# Bytes one more node (machine) costs beyond its buffer pairs, measured as
# (multi-node peak - one-node peak) / (nodes - 1) - its buffers, floored at 0.
set(NROS_ZEPHYR_HEAP_PARAM_NODE_REMAINDER 0 CACHE INTERNAL "issue 2780 heap floor")
set(NROS_ZEPHYR_HEAP_LIFECYCLE_MACHINE_REMAINDER 0 CACHE INTERNAL "issue 2780 heap floor")

# _nros_zephyr_heap_class_base(<lifecycle> <param_services> <out>)
#
# The one-node arm for an image of this class, in Kconfig's own order (the
# Cyclone arm is a stated 1 MiB and never reaches here), or "" when the image
# carries neither family.
function(_nros_zephyr_heap_class_base lifecycle params out)
    if(lifecycle)
        set(${out} "${NROS_ZEPHYR_HEAP_ONE_LIFECYCLE_MACHINE}" PARENT_SCOPE)
    elseif(params)
        set(${out} "${NROS_ZEPHYR_HEAP_ONE_PARAM_NODE}" PARENT_SCOPE)
    else()
        set(${out} "" PARENT_SCOPE)
    endif()
endfunction()

# _nros_zephyr_heap_floor(<base> <buffer> <out_floor> <out_composition>)
#
# <base> is the image's one-node arm (`_nros_zephyr_heap_class_base`);
# <buffer> the resolved PARAM_SERVICE_BUFFER_SIZE, or "" when no rung states
# it. Sets <out_floor> to the floor, or "" when the inventory counts neither a
# parameter-serving node nor a lifecycle machine (refused, absent, or a store-
# only image), so the caller falls back to <base> rather than derives.
function(_nros_zephyr_heap_floor base buffer out_floor out_comp)
    set(${out_floor} "" PARENT_SCOPE)
    set(${out_comp} "" PARENT_SCOPE)
    if(NOT "${base}" MATCHES "^[0-9]+$")
        return()
    endif()
    _nros_service_heap_count(NROS_ENTITY_PARAM_SERVICE_NODES _n)
    _nros_service_heap_count(NROS_ENTITY_LIFECYCLE_MACHINES _m)
    if(_n LESS 1 AND _m LESS 1)
        return()
    endif()
    _nros_service_heap_extra("${_n}" "${_m}" "${buffer}"
        "${NROS_ZEPHYR_HEAP_PARAM_NODE_REMAINDER}"
        "${NROS_ZEPHYR_HEAP_LIFECYCLE_MACHINE_REMAINDER}" _extra _comp)
    math(EXPR _floor "${base} + ${_extra}")
    set(${out_floor} "${_floor}" PARENT_SCOPE)
    set(${out_comp} "${base} for one node${_comp}" PARENT_SCOPE)
endfunction()
