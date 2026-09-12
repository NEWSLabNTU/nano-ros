# NanoRosPoolFloor.cmake — the C-array pool floor, in ONE place.
#
# Issues 1015 + 1033, and RFC-0100 D7:
#
#   > derivation publishes demand, unfloored; the floor lives at the consumer.
#
# A derived count is the image's DEMAND, and zero is a legitimate demand.
# Whether zero is a legal SIZE is a property of the STORAGE, so it is decided at
# each pool that names a knob — never in the shared derivation, which feeds
# consumers whose right answers at zero disagree:
#
#   zenoh   `queryable_entry_t queryables[ZPICO_MAX_QUERYABLES]` is a fixed C
#           array. A derived 0 gave a board that transmitted NOTHING in 15
#           seconds — no panic, no log line, core in WFI, every gate green,
#           because the number was derived correctly and delivered faithfully.
#
#   XRCE    the SAME derived numbers reach `NROS_XRCE_MAX_SUBSCRIBERS` and
#           `NROS_XRCE_MAX_SERVICE_SERVERS`, where zero is the ANSWER and worth
#           33,296 / 4,384 bytes of heap a slot.
#
#   uORB    `Slot g_pool[NROS_RMW_UORB_PX4_MAX_CALLBACKS]` and
#           `Entry g_table[NROS_RMW_UORB_REGISTRY_CAPACITY]` are C++, and ISO
#           C++ has no zero-size array at all — the TU is built `-Wpedantic`
#           inside a `-Werror` PX4, so zero is not a saving the language will
#           express (issue 1131).
#
# 1015's first fix floored the three zenoh pools in `EntityInventory::derive`.
# That landed the day BEFORE 1033's fix and silently defeated it, with every
# knob gate green. This file exists so the rule has one spelling on the CMake
# side rather than one per backend — CLAUDE.md's "add ONE shared helper rather
# than a second spelling". Its Rust sibling is
# `nros_cli_core::entity_inventory::c_array_pool_floor`.
#
# `check-c-array-pool-floors` holds both ends: every producer of a GUARDED knob
# calls this, and the derivation calls nothing.

include_guard(GLOBAL)

# _nros_c_array_pool_floor(<out_var> <value> <knob>)
#
# Raise a DERIVED demand to what a fixed C array can be sized to.
#
# EMPTY passes through untouched: empty means "nothing resolved", and turning
# that into a 1 would state a number where the reading build script is supposed
# to fall through to its own default.
#
# An explicit environment value is NOT floored — the callers apply this BEFORE
# their own resolve step, so a person who states 0 gets the `#error` beside the
# array that names the knob and its issue, rather than a number they did not ask
# for.
function(_nros_c_array_pool_floor out_var value knob)
    set(_v "${value}")
    if(_v MATCHES "^[0-9]+$" AND _v LESS 1)
        message(STATUS
            "nros: ${knob}=${_v} raised to 1 — it sizes a fixed C array, where "
            "zero is not a smaller pool (issue 1015)")
        set(_v 1)
    endif()
    set(${out_var} "${_v}" PARENT_SCOPE)
endfunction()
