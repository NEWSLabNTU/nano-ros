# NanoRosPx4ThreadStacks.cmake -- the threads a generated PX4 entry runs on must
# hold the executor's stack floor (issue 2398).
#
# A generated PX4 entry (RFC-0113 D9) runs nano-ros on TWO kinds of thread, and
# nano-ros sizes neither of them:
#
#   * the module's MAIN task. `<main> start` runs `task_spawn`, which opens the
#     session and every sched task's executor before it returns. On NuttX that
#     task is a builtin whose stack is the module's `STACK_MAIN` (PX4's default
#     is 2048 B; `platforms/nuttx/NuttX/CMakeLists.txt` writes it into the
#     builtin table). On POSIX the command runs on a thread of the PX4 server,
#     whose size `STACK_MAIN` does not set, so there is nothing to check.
#   * one PX4 WORK QUEUE per sched task (`px4::wq_configurations::<q>`, the queue
#     `nros_orchestration_ir::px4::px4_work_queues` picks). Its thread runs every
#     executor pass. The queue's stack is the BOARD's setting,
#     `CONFIG_WQ_<Q>_STACKSIZE`, and the queue is shared with PX4's own modules.
#
# Neither is raised here, and each is REFUSED below the floor, naming the knob:
#
#   * a work queue's stack belongs to the board config, which a module may not
#     change: it is a Kconfig `int` of PX4's `px4_work_queue`, and the queue is
#     shared with PX4's own modules. Fix: `CONFIG_WQ_<Q>_STACKSIZE` in the
#     board's `.px4board`.
#   * `STACK_MAIN` is this module's own argument, and NuttX gives it PX4's 2048 B
#     default when the module names none. Issue 0667's rule would have the port
#     RAISE an unset request to its floor, but here the floor is measured to be
#     about a quarter of what opening the session takes, so raising to it would
#     turn this refusal into a configure that passes and an image that overflows
#     its main task. A NuttX module therefore states STACK_MAIN itself.
#
# THE FLOOR is `NROS_EXECUTOR_MAIN_STACK_MIN` from the per-build config header
# the module compiles against: twice the executor value's size, derived by the
# build for the archive's own target. It is a NECESSARY condition, not a
# sufficient one. Issue 2398 measured the real frames on Cortex-M7 (thumbv7em,
# PX4 fmu-v6x's NuttX): opening the session is about four times the floor, and
# one executor pass is close to the floor before any backend or callback frame.
# A thread below the floor cannot work; a thread at the floor may still
# overflow, so size it from the measurement, not from this check.
#
# Pure functions of their arguments, so `scripts/check-px4-thread-stacks.sh`
# runs them under `cmake -P` with no PX4 tree.

include_guard(GLOBAL)

# PX4's own arithmetic for a work queue's thread stack
# (`platforms/common/px4_work_queue/WorkQueueManager.cpp`, `PX4_STACK_ADJUSTED`
# in `px4_platform_common/posix.h`): the configured size, scaled for 64-bit
# pointers, plus 24 KiB of overhead on POSIX. The rounding up to a page (POSIX)
# and to `PTHREAD_STACK_MIN` only ever grow it, so they are left out.
function(_nros_px4_effective_stack _out _platform _ptr_size _bytes)
    math(EXPR _scaled "${_bytes} * (${_ptr_size} / 4)")
    if(_platform STREQUAL "posix")
        math(EXPR _scaled "${_scaled} + 24576")
    elseif(NOT _platform STREQUAL "nuttx" AND NOT _platform STREQUAL "qurt")
        message(FATAL_ERROR "nros_px4_add_module: PX4_PLATFORM '${_platform}' is not "
            "one this module knows how PX4 sizes threads for (nuttx, posix, qurt)")
    endif()
    set(${_out} "${_scaled}" PARENT_SCOPE)
endfunction()

# The configured size whose effective stack (above) holds `_bytes`: what a user
# writes in the .px4board or as STACK_MAIN, so a remedy never names an
# effective size PX4 would scale again.
function(_nros_px4_configured_for _out _platform _ptr_size _bytes)
    set(_b "${_bytes}")
    if(_platform STREQUAL "posix")
        math(EXPR _b "${_b} - 24576")
        if(_b LESS 0)
            set(_b 0)
        endif()
    endif()
    math(EXPR _scale "${_ptr_size} / 4")
    math(EXPR _b "(${_b} + ${_scale} - 1) / ${_scale}")
    set(${_out} "${_b}" PARENT_SCOPE)
endfunction()

# The floor, read from the per-build C++ config header (issue 1824: the one the
# archive's own cargo run placed). A header with no such define is refused: it
# predates the define, so it is not the header of any archive this module can
# link.
function(nros_px4_read_stack_floor _out _header)
    if(NOT EXISTS "${_header}")
        message(FATAL_ERROR "nros_px4_add_module: config header not found:\n    ${_header}")
    endif()
    file(STRINGS "${_header}" _line
        REGEX "^#define NROS_EXECUTOR_MAIN_STACK_MIN [0-9]+")
    if(NOT _line)
        message(FATAL_ERROR "nros_px4_add_module: ${_header} defines no "
            "NROS_EXECUTOR_MAIN_STACK_MIN, so the executor's stack floor is unknown. "
            "Rebuild the archive and its placed headers together.")
    endif()
    list(GET _line 0 _line)
    string(REGEX REPLACE "^#define NROS_EXECUTOR_MAIN_STACK_MIN ([0-9]+).*" "\\1" _v "${_line}")
    set(${_out} "${_v}" PARENT_SCOPE)
endfunction()

# nros_px4_read_walked_floors(<prefix> <header>) -- the floors the placer's stack
# walk wrote into the same header (issue 2400 W3): `<prefix>_OPEN`,
# `<prefix>_SPIN` and their `_MEASURED` (1 = the image has an allowance row, so
# a thread below it is refused; 0 = unmeasured, so it is only reported). Each is
# empty when the archive was placed without a walk.
function(nros_px4_read_walked_floors _prefix _header)
    foreach(_role OPEN SPIN)
        foreach(_k "" _MEASURED)
            file(STRINGS "${_header}" _line
                REGEX "^#define NROS_STACK_FLOOR_${_role}${_k} [0-9]+")
            set(_v "")
            if(_line)
                list(GET _line 0 _line)
                string(REGEX REPLACE "^#define NROS_STACK_FLOOR_${_role}${_k} ([0-9]+).*" "\\1"
                    _v "${_line}")
            endif()
            set(${_prefix}_${_role}${_k} "${_v}" PARENT_SCOPE)
        endforeach()
    endforeach()
endfunction()

# The sched tasks of a generated entry and the work queue each runs on, as
# `<item>=<queue>` pairs, read from the entry's own constructor calls
# (`SchedTaskWorkItem("<item>", px4::wq_configurations::<queue>)`). Reading the
# generated file, not re-deriving the rank rule, keeps the queues this checks the
# queues the image runs on.
function(nros_px4_entry_work_queues _out _entry)
    file(STRINGS "${_entry}" _lines
        REGEX "SchedTaskWorkItem\\(\"[^\"]+\", px4::wq_configurations::[A-Za-z0-9_]+\\)")
    set(_pairs "")
    foreach(_l IN LISTS _lines)
        string(REGEX REPLACE
            ".*SchedTaskWorkItem\\(\"([^\"]+)\", px4::wq_configurations::([A-Za-z0-9_]+)\\).*"
            "\\1=\\2" _p "${_l}")
        list(APPEND _pairs "${_p}")
    endforeach()
    set(${_out} "${_pairs}" PARENT_SCOPE)
endfunction()

# nros_px4_check_thread_stacks(MODULE <m> PLATFORM <PX4_PLATFORM>
#     POINTER_SIZE <bytes> FLOOR <bytes> FLOOR_SOURCE <header>
#     QUEUES <item=queue>... [STACK_MAIN <value>]
#     [WALK_OPEN <bytes> WALK_OPEN_MEASURED <0|1> WALK_SPIN <bytes> WALK_SPIN_MEASURED <0|1>])
#
# Reads each queue's `CONFIG_WQ_<Q>_STACKSIZE` from the calling scope (PX4's
# `cmake/kconfig.cmake` sets every board option as a cache variable). Collects
# every shortfall and refuses once, naming all of them. STACK_MAIN is checked on
# NuttX only, the one platform where PX4 sizes a builtin's task from it.
#
# Two floors per thread. FLOOR (the executor's minimum) is always enforced. The
# WALK floors (issue 2400 W3) are the placer's measured stack walk: a work queue
# runs an executor pass (SPIN), the main task opens the session (OPEN). A
# MEASURED walk floor is enforced like FLOOR; an unmeasured one is reported as a
# warning and never refused (issue 2400 ruling A).
function(nros_px4_check_thread_stacks)
    cmake_parse_arguments(T ""
        "MODULE;PLATFORM;POINTER_SIZE;FLOOR;FLOOR_SOURCE;STACK_MAIN;WALK_OPEN;WALK_OPEN_MEASURED;WALK_SPIN;WALK_SPIN_MEASURED"
        "QUEUES" ${ARGN})
    set(_short "")
    set(_warn "")

    # The bytes a thread must hold and why, for one walk role.
    foreach(_role OPEN SPIN)
        set(_need_${_role} "${T_FLOOR}")
        set(_why_${_role} "the executor's minimum")
        if(NOT "${T_WALK_${_role}}" STREQUAL "" AND "${T_WALK_${_role}_MEASURED}" STREQUAL "1"
                AND T_WALK_${_role} GREATER T_FLOOR)
            set(_need_${_role} "${T_WALK_${_role}}")
            set(_why_${_role} "the measured stack floor of its deepest path")
        endif()
    endforeach()
    # An unmeasured walk floor above the stack: said, never refused.
    macro(_nros_px4_note_unmeasured _role _thread _eff _knob)
        if(NOT "${T_WALK_${_role}}" STREQUAL "" AND NOT "${T_WALK_${_role}_MEASURED}" STREQUAL "1"
                AND ${_eff} LESS T_WALK_${_role})
            string(APPEND _warn
                "  * ${_thread} has ${${_eff}} B, below the UNMEASURED stack floor "
                "${T_WALK_${_role}} B of its deepest path. Not enforced: the floor is a static "
                "walk that has not been measured on a running image. If it overflows, raise "
                "${_knob} first.\n")
        endif()
    endmacro()

    foreach(_pair IN LISTS T_QUEUES)
        string(REPLACE "=" ";" _kv "${_pair}")
        list(GET _kv 0 _item)
        list(GET _kv 1 _q)
        string(TOUPPER "${_q}" _Q)
        set(_opt "CONFIG_WQ_${_Q}_STACKSIZE")
        if(NOT DEFINED ${_opt} OR "${${_opt}}" STREQUAL "")
            string(APPEND _short
                "  * `${_item}` runs on PX4's work queue `wq:${_q}`, but the board config "
                "sets no ${_opt}, so its stack cannot be checked. This module was written "
                "against PX4 1.17, whose `px4_work_queue` Kconfig defines it.\n")
            continue()
        endif()
        _nros_px4_effective_stack(_eff "${T_PLATFORM}" "${T_POINTER_SIZE}" "${${_opt}}")
        if(_eff LESS _need_SPIN)
            _nros_px4_configured_for(_cfg "${T_PLATFORM}" "${T_POINTER_SIZE}" "${_need_SPIN}")
            string(APPEND _short
                "  * `${_item}` runs on PX4's work queue `wq:${_q}`, whose thread stack is "
                "${_eff} B (${_opt}=${${_opt}}), below ${_why_SPIN} (${_need_SPIN} B). The "
                "queue's stack is the board's setting: raise it with `${_opt}=${_cfg}` or more "
                "in the board's .px4board (PX4 accepts up to 10000).\n")
        else()
            _nros_px4_note_unmeasured(SPIN "`${_item}`'s work queue `wq:${_q}`" _eff "${_opt}")
        endif()
    endforeach()

    if(T_PLATFORM STREQUAL "nuttx")
        if("${T_STACK_MAIN}" STREQUAL "")
            string(APPEND _short
                "  * the module's main task opens the session and every executor in "
                "`start`, and this module passes no STACK_MAIN, so NuttX gives it PX4's "
                "default of 2048 B. Pass STACK_MAIN <bytes> to nros_px4_add_module, at "
                "least ${_need_OPEN} (${_why_OPEN}).\n")
        elseif(NOT "${T_STACK_MAIN}" MATCHES "^[0-9]+$")
            string(APPEND _short
                "  * STACK_MAIN is '${T_STACK_MAIN}', which is not a number of bytes, so "
                "it cannot be checked. Give nros_px4_add_module a plain byte count.\n")
        else()
            _nros_px4_effective_stack(_eff "${T_PLATFORM}" "${T_POINTER_SIZE}" "${T_STACK_MAIN}")
            if(_eff LESS _need_OPEN)
                _nros_px4_configured_for(_cfg "${T_PLATFORM}" "${T_POINTER_SIZE}" "${_need_OPEN}")
                string(APPEND _short
                    "  * the module's main task opens the session and every executor in "
                    "`start`, and its STACK_MAIN is ${T_STACK_MAIN} B, below ${_why_OPEN} "
                    "(${_need_OPEN} B). Pass STACK_MAIN ${_cfg} or more to nros_px4_add_module.\n")
            else()
                _nros_px4_note_unmeasured(OPEN "the module's main task" _eff "STACK_MAIN")
            endif()
        endif()
    endif()

    if("${T_WALK_OPEN}${T_WALK_SPIN}" STREQUAL "")
        message(STATUS "nros_px4_add_module(${T_MODULE}): the nano-ros archive was placed "
            "without a stack walk, so its threads are checked against the executor's minimum "
            "only. Rebuild it with the placer command this module names to check them against "
            "their stack floors.")
    endif()
    if(NOT _warn STREQUAL "")
        message(WARNING "nros_px4_add_module(${T_MODULE}): a thread this module runs nano-ros "
            "on is below its stack floor:\n${_warn}"
            "The floors are in\n    ${T_FLOOR_SOURCE}")
    endif()
    if(NOT _short STREQUAL "")
        message(FATAL_ERROR
            "nros_px4_add_module(${T_MODULE}): a thread this module runs nano-ros on has "
            "less stack than it needs:\n${_short}"
            "The executor's minimum, ${T_FLOOR} B, is NROS_EXECUTOR_MAIN_STACK_MIN, and the "
            "stack floors of the session open and of one executor pass are "
            "NROS_STACK_FLOOR_OPEN and NROS_STACK_FLOOR_SPIN, all in\n"
            "    ${T_FLOOR_SOURCE}\n"
            "A thread below its floor overflows, and the overflow corrupts whatever memory "
            "sits below the stack, so it surfaces later and somewhere else.")
    endif()
endfunction()
