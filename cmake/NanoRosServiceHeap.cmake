# NanoRosServiceHeap.cmake -- what the runtime's per-node service families cost
# on a port's heap, beyond the first node (issue 2780).
#
# The runtime allocates its own service servers from the platform heap, PER
# NODE:
#
#   - every node carrying the PARAMETER services runs six servers;
#   - every LIFECYCLE state machine runs five.
#
# Each server owns a request/reply pair of PARAM_SERVICE_BUFFER_SIZE bytes
# (nros-node's `LIFECYCLE_SERVICE_BUFFER_SIZE` is the same constant). A port's
# heap default prices ONE node and ONE machine; this is the rest:
#
#     extra = (param_nodes - 1) x (6 x 2 x buffer + param_remainder)
#           + (machines - 1)    x (5 x 2 x buffer + lifecycle_remainder)
#
# One node and one machine add nothing. The remainders are what one more node
# (machine) costs beyond its buffer pairs, MEASURED per port, so they are the
# caller's, as is the base the extra is added to. Shared by every port's heap
# derivation so the arithmetic has one spelling; the cargo road's twin lives in
# nros-sizing-descriptor and is checked against these constants.
#
# The constants are CACHE INTERNAL, not plain variables: this file is
# include_guard(GLOBAL), so if its first include runs inside a function frame
# (NanoRosEntry.cmake is reached through `_nros_import_once`), plain variables
# die when that frame pops and every later include is a guarded no-op -- the
# arithmetic would then run on empty values. The `_NROS_ENTRY_DIR` pattern.

include_guard(GLOBAL)

set(NROS_PARAM_SERVICES_PER_NODE 6 CACHE INTERNAL
    "parameter servers on every node that carries them (issue 2780)")
set(NROS_LIFECYCLE_SERVICES_PER_MACHINE 5 CACHE INTERNAL
    "lifecycle servers per state machine (issue 2780)")
# nros-node's PARAM_SERVICE_BUFFER_SIZE when no rung states one.
set(NROS_PARAM_SERVICE_DEFAULT_BUFFER 4096 CACHE INTERNAL
    "nros-node PARAM_SERVICE_BUFFER_SIZE when no rung states one (issue 2780)")

# _nros_service_heap_count(<var> <out>) -- an inventory count, or 0.
function(_nros_service_heap_count var out)
    if(DEFINED ${var} AND "${${var}}" MATCHES "^[0-9]+$")
        set(${out} "${${var}}" PARENT_SCOPE)
    else()
        set(${out} 0 PARENT_SCOPE)
    endif()
endfunction()

# _nros_service_heap_extra(<param_nodes> <machines> <buffer>
#                          <param_remainder> <lifecycle_remainder>
#                          <out_extra> <out_composition>)
#
# <buffer> is the stated PARAM_SERVICE_BUFFER_SIZE, or "" for nros-node's
# default. Sets <out_extra> to the bytes above the one-node, one-machine base
# (0 when neither count exceeds one), and <out_composition> to the arithmetic as
# a sentence for a configure STATUS line.
function(_nros_service_heap_extra n m buffer rem_p rem_l out_extra out_comp)
    if("${buffer}" MATCHES "^[0-9]+$" AND buffer GREATER 0)
        set(_buf "${buffer}")
        set(_buf_from "stated")
    else()
        set(_buf "${NROS_PARAM_SERVICE_DEFAULT_BUFFER}")
        set(_buf_from "nros-node's default")
    endif()
    set(_extra 0)
    set(_comp "")
    if(n GREATER 1)
        math(EXPR _per "${NROS_PARAM_SERVICES_PER_NODE} * 2 * ${_buf} + ${rem_p}")
        math(EXPR _extra "${_extra} + (${n} - 1) * ${_per}")
        string(APPEND _comp
            " + ${n}-1 parameter nodes x ${_per} (${NROS_PARAM_SERVICES_PER_NODE} servers"
            " x 2 x ${_buf} + ${rem_p})")
    endif()
    if(m GREATER 1)
        math(EXPR _per "${NROS_LIFECYCLE_SERVICES_PER_MACHINE} * 2 * ${_buf} + ${rem_l}")
        math(EXPR _extra "${_extra} + (${m} - 1) * ${_per}")
        string(APPEND _comp
            " + ${m}-1 lifecycle machines x ${_per} (${NROS_LIFECYCLE_SERVICES_PER_MACHINE} servers"
            " x 2 x ${_buf} + ${rem_l})")
    endif()
    string(APPEND _comp "; buffer ${_buf_from}")
    set(${out_extra} "${_extra}" PARENT_SCOPE)
    set(${out_comp} "${_comp}" PARENT_SCOPE)
endfunction()
