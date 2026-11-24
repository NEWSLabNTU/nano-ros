# nros_heap_floor.cmake -- the per-node floor of NROS_MEMORY_HEAP_BYTES (issue
# 2780, fix (b) of its ruling).
#
# The runtime's own service families allocate their servers from the platform
# heap, and they are PER NODE:
#
#   - every node carrying the PARAMETER services runs six servers;
#   - every LIFECYCLE state machine runs five.
#
# A parameter server owns a request/reply pair of PARAM_SERVICE_BUFFER_SIZE
# bytes; a lifecycle machine's servers own buffers sized for the lifecycle
# messages (NROS_LIFECYCLE_MACHINE_BUFFER_BYTES, nros-node
# lifecycle_services.rs, issue 2919). Kconfig's flat arms were each priced on ONE node:
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
# MEASURED 2026-11-22, native_sim/native/64, under a 327,680 arena: the C++
# derived-sched-tasks image (4 nodes) peaks at 139,616 and levels (3 nodes)
# at 105,712, so a node costs 33,904 -- under the 49,152 of buffers priced
# per node. The parameter remainder is therefore 0, and the derived floors
# (262,144 and 212,992) keep 122,528 and 107,280 over the peak, well above
# the 24,576-byte floor.
set(NROS_ZEPHYR_HEAP_PARAM_NODE_REMAINDER 0 CACHE INTERNAL "issue 2780 heap floor")
# Issue 2780 -- the lifecycle remainder is PER RMW, because on XRCE a machine
# costs more than its buffers. Its five servers are one allocation of their
# lifecycle buffers plus each server's handle, a 648-byte XRCE handle each.
# The buffers were 5 x 2 x 4096 = 40,960 B when the remainder below was
# measured; issue 2919 sized them for the lifecycle messages
# (NROS_LIFECYCLE_MACHINE_BUFFER_BYTES), and the remainder is the cost BEYOND
# them, so it still holds. And each server takes a service-server slot in
# `xrce_session_state_t`, which the configure sizes from the same inventory
# (NROS_XRCE_MAX_SERVICE_SERVERS, 19 -> 24 for the second machine), about
# 4.7 KiB a slot. MEASURED 2026-11-24, mps2/an385, Zephyr 3.7, two C talkers
# (host 175): one machine peaks at 183,312 and two at 250,896, so the second
# costs 67,584 = 40,960 of buffers + 26,624. Priced at 32,768: one sample, and
# the peaks miss the two /rosout writers the backend refused.
# zenoh has no measured remainder and keeps 0: its servers are queryables in
# a pool the image already carries, and the same two-machine image on zenoh
# peaked at 181,536 on the flat 196,608 arm, 105,184 under the floor derived
# for it here.
set(NROS_ZEPHYR_HEAP_LIFECYCLE_MACHINE_REMAINDER 0 CACHE INTERNAL "issue 2780 heap floor")
set(NROS_ZEPHYR_HEAP_LIFECYCLE_MACHINE_REMAINDER_XRCE 32768 CACHE INTERNAL
    "issue 2780 heap floor")

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

# _nros_zephyr_heap_floor(<base> <buffer> <rmw_xrce> <out_floor> <out_composition>)
#
# <base> is the image's one-node arm (`_nros_zephyr_heap_class_base`);
# <buffer> the resolved PARAM_SERVICE_BUFFER_SIZE, or "" when no rung states
# it; <rmw_xrce> true when the image's RMW is XRCE (its lifecycle remainder).
# Sets <out_floor> to the floor, or "" when the inventory counts neither a
# parameter-serving node nor a lifecycle machine (refused, absent, or a store-
# only image), so the caller falls back to <base> rather than derives.
function(_nros_zephyr_heap_floor base buffer rmw_xrce out_floor out_comp)
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
    set(_rem_l "${NROS_ZEPHYR_HEAP_LIFECYCLE_MACHINE_REMAINDER}")
    if(rmw_xrce)
        set(_rem_l "${NROS_ZEPHYR_HEAP_LIFECYCLE_MACHINE_REMAINDER_XRCE}")
    endif()
    _nros_service_heap_extra("${_n}" "${_m}" "${buffer}"
        "${NROS_ZEPHYR_HEAP_PARAM_NODE_REMAINDER}" "${_rem_l}" _extra _comp)
    math(EXPR _floor "${base} + ${_extra}")
    set(${out_floor} "${_floor}" PARENT_SCOPE)
    set(${out_comp} "${base} for one node${_comp}" PARENT_SCOPE)
endfunction()
