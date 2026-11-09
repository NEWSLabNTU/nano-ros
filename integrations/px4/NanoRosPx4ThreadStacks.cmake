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
#     QUEUES <item=queue>... [STACK_MAIN <value>])
#
# Reads each queue's `CONFIG_WQ_<Q>_STACKSIZE` from the calling scope (PX4's
# `cmake/kconfig.cmake` sets every board option as a cache variable). Collects
# every shortfall and refuses once, naming all of them. STACK_MAIN is checked on
# NuttX only, the one platform where PX4 sizes a builtin's task from it.
function(nros_px4_check_thread_stacks)
    cmake_parse_arguments(T "" "MODULE;PLATFORM;POINTER_SIZE;FLOOR;FLOOR_SOURCE;STACK_MAIN"
        "QUEUES" ${ARGN})
    set(_short "")

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
        if(_eff LESS T_FLOOR)
            string(APPEND _short
                "  * `${_item}` runs on PX4's work queue `wq:${_q}`, whose thread stack is "
                "${_eff} B (${_opt}=${${_opt}}). The queue's stack is the board's setting: "
                "raise it to at least ${T_FLOOR} with `${_opt}=<bytes>` in the board's "
                ".px4board (PX4 accepts up to 10000).\n")
        endif()
    endforeach()

    if(T_PLATFORM STREQUAL "nuttx")
        if("${T_STACK_MAIN}" STREQUAL "")
            string(APPEND _short
                "  * the module's main task opens the session and every executor in "
                "`start`, and this module passes no STACK_MAIN, so NuttX gives it PX4's "
                "default of 2048 B. Pass STACK_MAIN <bytes> to nros_px4_add_module, at "
                "least ${T_FLOOR}.\n")
        elseif(NOT "${T_STACK_MAIN}" MATCHES "^[0-9]+$")
            string(APPEND _short
                "  * STACK_MAIN is '${T_STACK_MAIN}', which is not a number of bytes, so "
                "it cannot be checked. Give nros_px4_add_module a plain byte count.\n")
        else()
            _nros_px4_effective_stack(_eff "${T_PLATFORM}" "${T_POINTER_SIZE}" "${T_STACK_MAIN}")
            if(_eff LESS T_FLOOR)
                string(APPEND _short
                    "  * the module's main task opens the session and every executor in "
                    "`start`, and its STACK_MAIN is ${T_STACK_MAIN} B. Pass STACK_MAIN "
                    "<bytes> of at least ${T_FLOOR} to nros_px4_add_module.\n")
            endif()
        endif()
    endif()

    if(NOT _short STREQUAL "")
        message(FATAL_ERROR
            "nros_px4_add_module(${T_MODULE}): a thread this module runs nano-ros on has "
            "less stack than the executor needs (issue 2398):\n${_short}"
            "The floor, ${T_FLOOR} B, is NROS_EXECUTOR_MAIN_STACK_MIN in\n"
            "    ${T_FLOOR_SOURCE}\n"
            "It is necessary, not sufficient: a thread below it overflows, and the overflow "
            "corrupts whatever memory sits below the stack, so it surfaces later and "
            "somewhere else. Issue 2398 has the measured frames to size from.")
    endif()
endfunction()
