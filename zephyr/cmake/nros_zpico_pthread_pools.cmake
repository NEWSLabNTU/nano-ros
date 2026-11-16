# nros_zpico_pthread_pools.cmake -- the POSIX mutex and condition-variable
# pools a zenoh image needs on Zephyr, stated ONCE.
# SPDX-License-Identifier: MIT OR Apache-2.0
#
# zenoh-pico gives every declared subscriber AND every declared queryable a
# callback-drop sync group, which takes one `pthread_mutex_init` and one
# `pthread_cond_init`. Zephyr's POSIX pools are static; past either one the
# declaration fails and the application sees -100 naming nothing. The
# constants are MEASURED (see the refusals in `nros_cargo_build.cmake`, which
# cite each run); a derivation of zenoh-pico's internal structure would break
# silently whenever upstream adds a sync group, so these stay a measured
# bound, not a model.
#
# Two readers, one formula:
#   * `nros_image_kconfig.cmake` (before Kconfig) turns the floors into the
#     pools' Kconfig DEFAULTS for an image `nros configure` resolved, so no
#     leaf conf has to hand-set a count that the next entity outgrows;
#   * `nros_cargo_build.cmake` (after Kconfig) refuses a pool an explicit conf
#     line set below the floor.

# zenoh-pico's fixed share of each pool, beside the per-entity term.
set(NROS_ZPICO_COND_FIXED 4)          # max(island 2, native_sim NSOS 4)
set(NROS_ZPICO_MUTEX_FIXED_SUBS 22)   # subscriber-only floor (mr-canhubk344)
set(NROS_ZPICO_MUTEX_FIXED 24)        # subscriber + queryable floor (island)
set(NROS_ZPICO_PTHREAD_HEADROOM 4)    # an entity added without re-measuring
# The module's own Kconfig defaults (`zephyr/Kconfig`, `if NROS_RMW_ZENOH`).
# A derived default never goes BELOW them: the floor is what zenoh-pico needs,
# and an application's own pthread objects come out of the same pools.
set(NROS_ZPICO_COND_DEFAULT 16)
set(NROS_ZPICO_MUTEX_DEFAULT 64)

# nros_zpico_pthread_floors(<subs> <qrys> <slots> <out_cond> <out_mutex>)
#
# <out_cond>: subs + qrys + fixed + headroom. <out_mutex>: the larger of the
# subscriber+queryable floor and, when <slots> (RMW subscriber slots) is not
# empty, the subscriber-only floor. <subs>/<qrys> are the zenoh tables' sizes
# AFTER their C-array floor of 1.
function(nros_zpico_pthread_floors _subs _qrys _slots _out_cond _out_mutex)
    math(EXPR _cond
         "${_subs} + ${_qrys} + ${NROS_ZPICO_COND_FIXED} + ${NROS_ZPICO_PTHREAD_HEADROOM}")
    math(EXPR _mutex
         "${_subs} + ${_qrys} + ${NROS_ZPICO_MUTEX_FIXED} + ${NROS_ZPICO_PTHREAD_HEADROOM}")
    if(NOT "${_slots}" STREQUAL "")
        math(EXPR _mutex_s
             "${_slots} + ${NROS_ZPICO_MUTEX_FIXED_SUBS} + ${NROS_ZPICO_PTHREAD_HEADROOM}")
        if(_mutex_s GREATER _mutex)
            set(_mutex ${_mutex_s})
        endif()
    endif()
    set(${_out_cond} ${_cond} PARENT_SCOPE)
    set(${_out_mutex} ${_mutex} PARENT_SCOPE)
endfunction()

# nros_zpico_pthread_pool_kconfig(<resolved.cmake or ""> <out_file> [<knob_defaults>])
#
# Write <out_file> (`osource`d by `zephyr/Kconfig` before the module's own
# defaults) with the pools' derived defaults, under `if NROS_RMW_ZENOH`, from
# the counts `nros configure` resolved. A table the image STATES as a knob
# (`Kconfig.from-system-toml`, <knob_defaults>: `[image.<id>.knobs.zenoh]
# max_subscribers` / `max_queryables`) is sized at the larger of the two, as
# the zenoh tables themselves are. With no resolved fragment the file is
# removed, so a guest, or an image that stopped resolving, keeps the module
# default. Write-if-different: the file is a Kconfig source, and rewriting it
# unchanged on every configure would re-run Kconfig for nothing.
function(nros_zpico_pthread_pool_kconfig _resolved _out_file)
    set(_knobs "")
    if(ARGC GREATER 2 AND EXISTS "${ARGV2}")
        file(READ "${ARGV2}" _knobs)
    endif()
    if("${_resolved}" STREQUAL "" OR NOT EXISTS "${_resolved}")
        if(EXISTS "${_out_file}")
            file(REMOVE "${_out_file}")
        endif()
        return()
    endif()
    # Read in this function's frame, so nothing leaks into the hook's scope.
    include("${_resolved}")
    if(NOT DEFINED NROS_DERIVED_MAX_SUBSCRIBERS OR "${NROS_DERIVED_MAX_SUBSCRIBERS}" STREQUAL ""
       OR NOT DEFINED NROS_DERIVED_MAX_QUERYABLES OR "${NROS_DERIVED_MAX_QUERYABLES}" STREQUAL "")
        if(EXISTS "${_out_file}")
            file(REMOVE "${_out_file}")
        endif()
        return()
    endif()
    set(_subs ${NROS_DERIVED_MAX_SUBSCRIBERS})
    set(_qrys ${NROS_DERIVED_MAX_QUERYABLES})
    # The zenoh tables' C-array floor (`_nros_c_array_pool_floor`): a derived 0
    # is sized 1, so the pools are taken over the same numbers.
    if(_subs LESS 1)
        set(_subs 1)
    endif()
    if(_qrys LESS 1)
        set(_qrys 1)
    endif()
    # A stated table (`config NROS_MAX_<T>` with a `default` row).
    foreach(_t SUBSCRIBERS QUERYABLES)
        if(_knobs MATCHES "\nconfig NROS_MAX_${_t}\n    int\n(    depends on [^\n]*\n)*    default ([0-9]+)\n")
            set(_stated ${CMAKE_MATCH_2})
            if(_t STREQUAL "SUBSCRIBERS" AND _stated GREATER _subs)
                set(_subs ${_stated})
            elseif(_t STREQUAL "QUERYABLES" AND _stated GREATER _qrys)
                set(_qrys ${_stated})
            endif()
        endif()
    endforeach()
    set(_slots "")
    if(DEFINED NROS_DERIVED_RMW_SUBSCRIBER_SLOTS)
        set(_slots "${NROS_DERIVED_RMW_SUBSCRIBER_SLOTS}")
    endif()
    nros_zpico_pthread_floors(${_subs} ${_qrys} "${_slots}" _cond _mutex)
    if(_cond LESS NROS_ZPICO_COND_DEFAULT)
        set(_cond ${NROS_ZPICO_COND_DEFAULT})
    endif()
    if(_mutex LESS NROS_ZPICO_MUTEX_DEFAULT)
        set(_mutex ${NROS_ZPICO_MUTEX_DEFAULT})
    endif()
    set(_text
"# GENERATED by the nano-ros Zephyr module (nros_zpico_pthread_pools.cmake)
# -- do not edit. Re-rendered on every configure from ${_resolved}.
#
# The POSIX pools zenoh-pico needs for ${_subs} subscriber(s) and ${_qrys}
# queryable(s), as DEFAULTS: an explicit CONFIG_MAX_PTHREAD_*_COUNT line in a
# conf file still wins, and the build refuses one below the floor.

if NROS_RMW_ZENOH

configdefault MAX_PTHREAD_COND_COUNT
    default ${_cond}

configdefault MAX_PTHREAD_MUTEX_COUNT
    default ${_mutex}

endif # NROS_RMW_ZENOH
")
    set(_old "")
    if(EXISTS "${_out_file}")
        file(READ "${_out_file}" _old)
    endif()
    if(NOT _old STREQUAL _text)
        get_filename_component(_d "${_out_file}" DIRECTORY)
        file(MAKE_DIRECTORY "${_d}")
        file(WRITE "${_out_file}" "${_text}")
    endif()
    message(STATUS "nano-ros: zenoh POSIX pools derived: "
        "MAX_PTHREAD_COND_COUNT=${_cond}, MAX_PTHREAD_MUTEX_COUNT=${_mutex} "
        "(${_subs} subscriber(s), ${_qrys} queryable(s))")
endfunction()
