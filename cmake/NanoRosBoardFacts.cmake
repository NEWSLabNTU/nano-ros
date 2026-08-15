# phase-351 W5 — deliver one deploy's resolved board FACTS + SITE config to the
# cargo invocations this configure owns.
#
# RFC-0072 §5 splits board information into A (board facts, in the board
# package) and B (site config, in the user's `[deploy.<name>.nros]`). W1–W4 gave
# both a home and a validity domain. Delivery is this file.
#
# WHY THE INVOKER. Cargo discovers config from the invocation CWD upward, and
# Corrosion runs cargo from `workspace_toml_dir` — so a workspace MEMBER's own
# `.cargo/config.toml` is never read (phase-349 W2.0 measured it; that is why
# the `NROS_BOARD_TOML` row that wave wrote could not reach the members it was
# written for, and why phase-351 W6 retires it). The process environment does
# cross that boundary, and its owner is whoever spawns cargo.
#
# WHY VIA THE CLI. The resolution needs the board catalog, the deploy's site
# block, `{env:…}`/`{sdk.…}` interpolation and W4's netstack domain check. That
# is `nros ws board-facts` — one implementation, shared with every other lane,
# rather than a second one in cmake that would drift (the `ws model-dims` seam).
#
# NOT `set(ENV{...})`. Issue 0460: that touches only the configure-time process,
# so a knob published that way reaches the C lane (which re-bakes its command)
# and NOT the cargo one. `corrosion_set_env_vars` attaches to the target's own
# build command, which is what actually runs cargo.

include_guard(GLOBAL)

# nros_resolve_board_facts([BOARD <name>] [WORKSPACE <dir>])
#
# Resolve once per configure into the `NROS_BOARD_FACTS_ENV` cache entry (a
# `KEY=VALUE;…` list). Exactly one board is active per configure — the board
# module is selected by `if/elseif` on `NANO_ROS_BOARD` and the toolchain file
# must precede `project()` — so one cached value is the whole answer, never a
# table a caller has to index.
function(nros_resolve_board_facts)
    cmake_parse_arguments(_A "" "BOARD;WORKSPACE" "" ${ARGN})

    if(DEFINED NROS_BOARD_FACTS_ENV)
        return()
    endif()

    set(_board "${_A_BOARD}")
    if(_board STREQUAL "")
        set(_board "${NANO_ROS_BOARD}")
    endif()
    set(_ws "${_A_WORKSPACE}")
    if(_ws STREQUAL "")
        set(_ws "${NROS_WORKSPACE_DIR}")
    endif()
    if(_ws STREQUAL "")
        set(_ws "${CMAKE_SOURCE_DIR}")
    endif()

    # No board, or no CLI: deliver NOTHING and say so once. A host build has no
    # board rung to carry, and a tree without the in-tree CLI cannot resolve one
    # — both are legitimate, and both must be visible rather than looking like a
    # successful delivery of an empty set.
    if(_board STREQUAL "")
        set(NROS_BOARD_FACTS_ENV "" CACHE INTERNAL "phase-351 W5: no board in play")
        return()
    endif()
    if(NOT DEFINED _NANO_ROS_CODEGEN_TOOL OR NOT EXISTS "${_NANO_ROS_CODEGEN_TOOL}")
        message(STATUS
            "nano-ros: board facts NOT delivered for `${_board}` — no nros CLI "
            "(build it with `just setup-cli`).")
        set(NROS_BOARD_FACTS_ENV "" CACHE INTERNAL "phase-351 W5: no CLI")
        return()
    endif()

    execute_process(
        COMMAND "${_NANO_ROS_CODEGEN_TOOL}" ws board-facts "${_ws}" --board "${_board}"
        OUTPUT_VARIABLE _out
        ERROR_VARIABLE _err
        RESULT_VARIABLE _rc
        OUTPUT_STRIP_TRAILING_WHITESPACE)

    if(NOT _rc EQUAL 0)
        # A deploy that names a board nano-ros cannot resolve, or a netstack
        # outside that board's domain, is a CONFIGURATION error — failing here
        # is the whole point of W4. What is NOT an error is a workspace with no
        # matching deploy block: the same board module configures host-side
        # helper builds too.
        if(_err MATCHES "no \\[deploy" OR _err MATCHES "no system.toml")
            set(NROS_BOARD_FACTS_ENV "" CACHE INTERNAL "phase-351 W5: no deploy for this board")
            return()
        endif()
        message(FATAL_ERROR
            "nano-ros: could not resolve board facts for `${_board}` in ${_ws}:\n${_err}")
    endif()

    string(REPLACE "\n" ";" _lines "${_out}")
    list(REMOVE_ITEM _lines "")
    set(NROS_BOARD_FACTS_ENV "${_lines}" CACHE INTERNAL
        "phase-351 W5: resolved board facts + site config for ${_board}")
    list(LENGTH _lines _n)
    message(STATUS "nano-ros: board facts for `${_board}` — ${_n} value(s) delivered to cargo")
endfunction()

# nros_board_facts_env(<target>)
#
# Attach the resolved facts to a Corrosion target's cargo invocation. Sibling of
# `nros_cargo_profile_env`, and called from the same places for the same reason:
# the crate cannot read them from anywhere else.
function(nros_board_facts_env _target)
    nros_resolve_board_facts()
    if(NROS_BOARD_FACTS_ENV STREQUAL "")
        return()
    endif()
    if(NOT COMMAND corrosion_set_env_vars)
        message(FATAL_ERROR "nros_board_facts_env(${_target}): Corrosion not loaded")
    endif()
    corrosion_set_env_vars(${_target} ${NROS_BOARD_FACTS_ENV})
endfunction()
