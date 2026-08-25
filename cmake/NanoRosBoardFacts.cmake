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

# issue 0657 — `nros_corrosion_env_target`.
include("${CMAKE_CURRENT_LIST_DIR}/NanoRosCorrosionEnv.cmake")

# nros_resolve_board_facts([BOARD <name>] [DEPLOY <name>] [WORKSPACE <dir>])
#
# Resolve into `NROS_BOARD_FACTS_ENV` (a `KEY=VALUE;…` list) in the CALLER's
# scope, memoised per (board, deploy) so the verb runs once per distinct
# question rather than once per caller.
#
# Exactly one BOARD is active per configure — the board module is selected by
# `if/elseif` on `NANO_ROS_BOARD` and the toolchain file must precede
# `project()`. DEPLOY is NOT bound that way: one configure can carry several
# entry leaves, each naming its own deploy in its `package.xml` export tuple,
# so the memo is keyed on both rather than assuming one answer (issue 0755).
function(nros_resolve_board_facts)
    cmake_parse_arguments(_A "" "BOARD;DEPLOY;WORKSPACE" "" ${ARGN})

    set(_board "${_A_BOARD}")
    if(_board STREQUAL "")
        set(_board "${NANO_ROS_BOARD}")
    endif()

    # issue 0755 — the caller that knows the deploy is the entry verb, which
    # already resolved it (explicit `DEPLOY`, else the package.xml tuple
    # `find_package(nano_ros)` parsed into `NROS_DEPLOY`). Forwarding it is
    # what keeps a multi-deploy `system.toml` from becoming an AMBIGUITY
    # refusal — which this file's deliberately-soft error handling would then
    # turn into a silent skip, dropping the tier/sizing facts out of the image
    # with a STATUS line at best.
    set(_deploy "${_A_DEPLOY}")
    if(_deploy STREQUAL "" AND DEFINED NROS_DEPLOY)
        set(_deploy "${NROS_DEPLOY}")
    endif()

    # One cache entry per question asked. `_` is not legal in a cache name
    # position for arbitrary board/deploy spellings, so sanitise.
    string(MAKE_C_IDENTIFIER "NROS_BOARD_FACTS_ENV__${_board}__${_deploy}" _memo)
    if(DEFINED ${_memo})
        set(NROS_BOARD_FACTS_ENV "${${_memo}}" PARENT_SCOPE)
        return()
    endif()

    # Where to resolve FROM, most specific first. `APPLICATION_SOURCE_DIR` is
    # the Zephyr arm: that lane never sets `NANO_ROS_BOARD` (it names boards the
    # Zephyr way), but its application dir IS an entry leaf, which carries
    # `[package.metadata.nros.entry] deploy` — the second site home `nros ws
    # board-facts` reads. Without this the Zephyr lane resolved nothing at all.
    set(_ws "${_A_WORKSPACE}")
    foreach(_cand "${NROS_WORKSPACE_DIR}" "${APPLICATION_SOURCE_DIR}" "${CMAKE_SOURCE_DIR}")
        if(_ws STREQUAL "" AND NOT _cand STREQUAL "")
            set(_ws "${_cand}")
        endif()
    endforeach()

    if(NOT DEFINED _NANO_ROS_CODEGEN_TOOL OR NOT EXISTS "${_NANO_ROS_CODEGEN_TOOL}")
        message(STATUS
            "nano-ros: board facts NOT delivered — no nros CLI (build it with "
            "`./scripts/bootstrap.sh`; contributors: `just setup-cli`).")
        set(${_memo} "" CACHE INTERNAL "phase-351 W5: no CLI")
        set(NROS_BOARD_FACTS_ENV "" PARENT_SCOPE)
        return()
    endif()
    if(_ws STREQUAL "")
        message(STATUS
            "nano-ros: board facts NOT delivered — no workspace/application dir "
            "to resolve from (pass WORKSPACE).")
        set(${_memo} "" CACHE INTERNAL "phase-351 W5: no workspace")
        set(NROS_BOARD_FACTS_ENV "" PARENT_SCOPE)
        return()
    endif()

    # No `--board` when the lane does not know one: the verb then resolves the
    # dir's own deploy, which is exactly right for an entry leaf and for a
    # single-deploy workspace, and reports an ambiguity rather than guessing.
    set(_args ws board-facts "${_ws}")
    if(NOT _board STREQUAL "")
        list(APPEND _args --board "${_board}")
    endif()
    if(NOT _deploy STREQUAL "")
        list(APPEND _args --deploy "${_deploy}")
    endif()

    execute_process(
        COMMAND "${_NANO_ROS_CODEGEN_TOOL}" ${_args}
        OUTPUT_VARIABLE _out
        ERROR_VARIABLE _err
        RESULT_VARIABLE _rc
        OUTPUT_STRIP_TRAILING_WHITESPACE)

    if(NOT _rc EQUAL 0)
        # INVERTED on purpose (three tries taught this): the only failure that
        # is a CONFIGURATION error is a deploy asking for something its board
        # cannot do — W4's netstack domain. Everything else means "this build
        # has no board facts to carry", which the tree has always allowed:
        #   * `[deploy.native]` declares no `board` at all (host deploys);
        #   * no descriptor claims the board's spelling (issue 0606);
        #   * the dir declares no deploy (host-side helper builds).
        # Enumerating the skips got it wrong twice — each miss turned a normal
        # build into a FATAL — so the rule is now stated the other way round:
        # fail only on what we can name as wrong, say so and continue otherwise.
        if(_err MATCHES "does not support netstack" OR _err MATCHES "supported_netstacks")
            message(FATAL_ERROR
                "nano-ros: this deploy asks for a netstack its board does not "
                "support (phase-351 W4):\n${_err}")
        endif()
        string(REGEX REPLACE "\n+" " " _why "${_err}")
        string(SUBSTRING "${_why}" 0 200 _why)
        message(STATUS
            "nano-ros: board facts NOT delivered from ${_ws} — ${_why}")
        set(${_memo} "" CACHE INTERNAL "phase-351 W5: nothing to deliver")
        set(NROS_BOARD_FACTS_ENV "" PARENT_SCOPE)
        return()
    endif()

    string(REPLACE "\n" ";" _lines "${_out}")
    list(REMOVE_ITEM _lines "")
    set(${_memo} "${_lines}" CACHE INTERNAL
        "phase-351 W5: resolved board facts + site config from ${_ws}")
    set(NROS_BOARD_FACTS_ENV "${_lines}" PARENT_SCOPE)
    list(LENGTH _lines _n)
    message(STATUS "nano-ros: board facts from ${_ws} — ${_n} value(s) delivered to cargo")
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
    # issue 0657 — attach to the target the cargo command actually READS.
    # Corrosion 0.6 makes `<crate>` (INTERFACE, carries the env genex) and
    # `<crate>-static` (IMPORTED, just names the .a); `set_property` succeeds on
    # both and only the first is consumed. Every call site here passed the
    # `-static` spelling, so this wave's whole point — the board rung reaching
    # cargo — was landing on a property nothing reads.
    nros_corrosion_env_target("${_target}" _target)
    corrosion_set_env_vars(${_target} ${NROS_BOARD_FACTS_ENV})
endfunction()
