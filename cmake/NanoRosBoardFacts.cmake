# phase-351 W5 — deliver one deploy's resolved board FACTS + SITE config to the
# cargo invocations this configure owns.
#
# RFC-0072 §5 splits board information into A (board facts, in the board
# package) and B (site config, in the user's `[board_config.<board>]`). W1–W4 gave
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
# issue 1712 — the image's own `[image.<id>] env`, on the same targets.
include("${CMAKE_CURRENT_LIST_DIR}/NanoRosImageEnv.cmake")
# issue 1263 -- `nros_resolve_cli`, the one lookup for the CLI. The Zephyr lane
# calls `nros_resolve_board_facts()` before anything else has loaded it
# (zephyr/CMakeLists.txt, right after nros_cargo_build.cmake), so a
# `COMMAND nros_resolve_cli` guard would skip on exactly the lane that needs it.
include("${CMAKE_CURRENT_LIST_DIR}/NanoRosCodegenCore.cmake")
# issue 1263 -- the checkout this file belongs to, for `--nano-ros-path`.
# Resolved here, at file scope: inside the function CMAKE_CURRENT_LIST_DIR
# names the CALLER. The CLI otherwise searches upward from the entry dir, which
# finds nothing for a downstream project whose nano-ros sits below it
# (third-party/nano-ros), and NROS_REPO_DIR is set only after the Zephyr lane
# has already asked.
get_filename_component(_nros_board_facts_repo "${CMAKE_CURRENT_LIST_DIR}/.." ABSOLUTE)
set(_NROS_BOARD_FACTS_REPO "${_nros_board_facts_repo}" CACHE INTERNAL
    "nano-ros checkout that owns NanoRosBoardFacts.cmake (issue 1263)")

# nros_resolve_board_facts([BOARD <name>] [DEPLOY <name>] [WORKSPACE <dir>])
#
# Resolve into `NROS_BOARD_FACTS_ENV` (a `KEY=VALUE;…` list) in the CALLER's
# scope, memoised per (board, deploy) so the verb runs once per distinct
# question rather than once per caller.
#
# Exactly one BOARD is active per configure — the board module is selected by
# `if/elseif` on `NROS_BOARD` and the toolchain file must precede
# `project()`. DEPLOY is NOT bound that way: one configure can carry several
# entry leaves, each naming its own deploy in its `package.xml` export tuple,
# so the memo is keyed on both rather than assuming one answer (issue 0755).
function(nros_resolve_board_facts)
    cmake_parse_arguments(_A "" "BOARD;DEPLOY;WORKSPACE" "" ${ARGN})

    set(_board "${_A_BOARD}")
    if(_board STREQUAL "")
        set(_board "${NROS_BOARD}")
    endif()

    # issue 0755 — the caller that knows the deploy is the entry verb, which
    # already resolved it (explicit `DEPLOY`, else the package.xml tuple
    # `find_package(nano_ros)` parsed into `NROS_DEPLOY`). Forwarding it is
    # what keeps a multi-deploy `system.toml` from becoming an AMBIGUITY
    # refusal — which this file used to turn into a silent skip, dropping the
    # tier/sizing facts out of the image with a STATUS line at best (a
    # configure error since issue 0941).
    set(_deploy "${_A_DEPLOY}")
    # phase-484 W2b — a standalone leaf's `NROS_DEPLOY` is its PLATFORM token
    # (`threadx`, `freertos`), and `board-facts --deploy` matches the KEYS of
    # the leaf's `[image.*]`/`[deploy.*]` tables (`threadx-linux`). Forwarding
    # the platform made every C/C++ leaf an "no [image.threadx]" refusal,
    # which this file soft-skipped (until issue 0941) — latent until W2b moved FREERTOS_PORT and
    # THREADX_CONFIG_DIR off the repo-wide export and onto exactly this
    # delivery, at which point `zpico-sys` panicked on every such leaf. The
    # leaf's resolved image id names the right row.
    if(_deploy STREQUAL "" AND DEFINED NROS_LEAF_IMAGE_ID)
        set(_deploy "${NROS_LEAF_IMAGE_ID}")
    endif()
    if(_deploy STREQUAL "" AND DEFINED NROS_DEPLOY)
        set(_deploy "${NROS_DEPLOY}")
    endif()

    # One cache entry per question asked. `_` is not legal in a cache name
    # position for arbitrary board/deploy spellings, so sanitise.
    string(MAKE_C_IDENTIFIER "NROS_BOARD_FACTS_ENV__${_board}__${_deploy}" _memo)
    # issue 1263 -- only a RESOLVED answer outlives the configure that found it.
    # A failure ("no CLI", no workspace, the CLI refusing) describes the build
    # environment of one configure, not the board, and these used to be cached
    # too: with the lookup fixed, every build dir configured before still got
    # an empty answer here, before any check ran, and said nothing. A cached
    # failure from an older configure is dropped; this run's failures live in a
    # GLOBAL property, so the several callers of one configure still ask once.
    # phase-484 W2b — and a RESOLVED answer no longer outlives it either. The
    # cached one had no freshness input: it survived a rebuilt `nros` and an
    # edited `nros-board.toml` across every reconfigure, so a board fact added
    # to the descriptor never reached the build (measured: FREERTOS_PORT absent
    # from a workspace C image whose descriptor stated it). One CLI call per
    # (board, deploy) per configure is what the memo is for; a cache entry an
    # older checkout wrote is dropped on sight.
    if(DEFINED CACHE{${_memo}})
        unset(${_memo} CACHE)
    endif()
    get_property(_memo_hit GLOBAL PROPERTY ${_memo}__VALUE SET)
    if(_memo_hit)
        get_property(_memo_val GLOBAL PROPERTY ${_memo}__VALUE)
        set(NROS_BOARD_FACTS_ENV "${_memo_val}" PARENT_SCOPE)
        return()
    endif()
    get_property(_tried GLOBAL PROPERTY ${_memo} SET)
    if(_tried)
        set(NROS_BOARD_FACTS_ENV "" PARENT_SCOPE)
        return()
    endif()

    # Where to resolve FROM, most specific first. `APPLICATION_SOURCE_DIR` is
    # the Zephyr arm: that lane never sets `NROS_BOARD` (it names boards the
    # Zephyr way), but its application dir IS an entry leaf, and `nros ws
    # board-facts` resolves an entry's board from the bringup image that claims
    # it (`leaf_system::for_entry`, phase-445 W5 — it was the entry's retired
    # `[package.metadata.nros.entry] deploy`). Without this the Zephyr lane
    # resolved nothing at all.
    #
    # Issue 1827 — `NROS_BRINGUP_DIR` (set by `nros build`) comes before the
    # workspace: the workspace answers only while it has ONE bringup, and the
    # verb refuses a second as ambiguous (a configure error since issue 0941).
    set(_ws "${_A_WORKSPACE}")
    foreach(_cand "${NROS_BRINGUP_DIR}" "${NROS_WORKSPACE_DIR}" "${APPLICATION_SOURCE_DIR}" "${CMAKE_SOURCE_DIR}")
        if(_ws STREQUAL "" AND NOT _cand STREQUAL "")
            set(_ws "${_cand}")
        endif()
    endforeach()

    # issue 1263 -- ask the shared resolver, not one cache name. The CLI lives
    # under `_NROS_ZEPHYR_CODEGEN_TOOL` on the Zephyr lane and
    # `_NANO_ROS_CODEGEN_TOOL` elsewhere (NanoRosImageAgreement.cmake records
    # why a check knowing only one "would silently do nothing on the lane it
    # was written for"), and on Zephyr neither is set yet when this runs, so
    # every Zephyr image printed the line below and skipped its board facts.
    nros_resolve_cli(_nros OPTIONAL CONTEXT "nros_resolve_board_facts")
    if(NOT _nros OR NOT EXISTS "${_nros}")
        # Issue 0941 — without the CLI nothing can say whether this configure
        # has facts to carry. One that states a deployment (a `system.toml` in
        # scope) or names a board does, and skipping it is the silent half-build
        # this function used to produce; a guest project with neither has none.
        file(GLOB _nros_bf_sys "${_ws}/system.toml" "${_ws}/src/*/system.toml")
        if(_nros_bf_sys OR NOT _board STREQUAL "")
            message(FATAL_ERROR
                "nano-ros: board facts cannot be resolved for ${_ws} — no nros CLI "
                "(build it with `./scripts/bootstrap.sh`; contributors: "
                "`just setup-cli`).")
        endif()
        message(STATUS
            "nano-ros: board facts — none for this configure (no nros CLI, and "
            "${_ws} states no deployment and names no board).")
        set_property(GLOBAL PROPERTY ${_memo} "")
        set(NROS_BOARD_FACTS_ENV "" PARENT_SCOPE)
        return()
    endif()

    # No `--board` when the lane does not know one: the verb then resolves the
    # dir's own deploy, which is exactly right for an entry leaf and for a
    # single-deploy workspace, and reports an ambiguity rather than guessing.
    set(_args ws board-facts "${_ws}" --nano-ros-path "${_NROS_BOARD_FACTS_REPO}")
    if(NOT _board STREQUAL "")
        list(APPEND _args --board "${_board}")
    endif()
    if(NOT _deploy STREQUAL "")
        list(APPEND _args --deploy "${_deploy}")
    endif()

    execute_process(
        COMMAND "${_nros}" ${_args}
        OUTPUT_VARIABLE _out
        ERROR_VARIABLE _err
        RESULT_VARIABLE _rc
        OUTPUT_STRIP_TRAILING_WHITESPACE)

    # Issue 0941 — the VERB decides whether this configure has facts to carry:
    # "nothing to carry" is exit 0 with no output and its reason on stderr, and
    # every nonzero exit is an error. So there is no list of skippable messages
    # here any more. The two earlier hardenings (`21be9c0684`, `5d9abbd376`)
    # each broke a lane because they matched the verb's error TEXT; the
    # inverted rule that replaced them stayed silent on everything else,
    # including twelve unreachable site blocks (issue 0940), and since
    # phase-484 W2b a cross image that skips here dies later in `zpico-sys`
    # on a missing `FREERTOS_PORT`.
    if(NOT _rc EQUAL 0)
        set(_nros_bf_asked "")
        if(NOT _board STREQUAL "")
            string(APPEND _nros_bf_asked " --board ${_board}")
        endif()
        if(NOT _deploy STREQUAL "")
            string(APPEND _nros_bf_asked " --deploy ${_deploy}")
        endif()
        message(FATAL_ERROR
            "nano-ros: board facts for ${_ws}${_nros_bf_asked} could not be "
            "resolved (`nros ws board-facts`):\n${_err}")
    endif()
    if(_out STREQUAL "")
        string(REGEX REPLACE "\n+" " " _why "${_err}")
        message(STATUS "nano-ros: board facts — none for this configure: ${_why}")
        set_property(GLOBAL PROPERTY ${_memo} "")
        set(NROS_BOARD_FACTS_ENV "" PARENT_SCOPE)
        return()
    endif()

    string(REPLACE "\n" ";" _lines "${_out}")
    list(REMOVE_ITEM _lines "")
    set_property(GLOBAL PROPERTY ${_memo}__VALUE "${_lines}")
    # Issue 2204 — the question that resolved, so a later check asks it of the
    # same deploy (`nros_check_zephyr_rust_target`).
    set_property(GLOBAL PROPERTY NROS_BOARD_FACTS_RESOLVED_CMD "${_nros};${_args}")
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
    # Issue 1712 — the image rung rides the board rung's call sites: this is
    # the one helper every Corrosion import must call
    # (`check-board-facts-delivery`), so a cargo target that gets the board
    # gets the image too. Before the early return, because an image may state
    # rows on a board with no facts to carry.
    nros_image_env_attach("${_target}")
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

# nros_board_facts_env_deferred(<target>)
#
# issue 1541 — `nros_board_facts_env`, scheduled for the END of the top-level
# scope, once per target. For a Corrosion import that runs BEFORE the configure
# knows its deploy: nros-c's and nros-cpp's `CMakeLists.txt` are added by the
# root project during `nano_ros_workspace()`, while `NROS_DEPLOY` (which
# `nros_resolve_board_facts` forwards, issue 0755) is parsed later, by the
# entry's `find_package(nano_ros)`. Resolving inline would ask the question
# with half its inputs and memoise the answer.
#
# Same shape as `nros_entity_facts_env_deferred` (NanoRosEntityFacts.cmake),
# and for the same two measured reasons: DEFER to `CMAKE_SOURCE_DIR`, not the
# current directory, and carry the targets in a GLOBAL property because a
# deferred CALL argument arrives empty.
function(nros_board_facts_env_deferred _target)
    get_property(_queued GLOBAL PROPERTY NROS_BOARD_FACTS_TARGETS)
    if("${_target}" IN_LIST _queued)
        return()
    endif()
    set_property(GLOBAL APPEND PROPERTY NROS_BOARD_FACTS_TARGETS "${_target}")
    get_property(_scheduled GLOBAL PROPERTY NROS_BOARD_FACTS_FLUSH_SCHEDULED)
    if(_scheduled)
        return()
    endif()
    set_property(GLOBAL PROPERTY NROS_BOARD_FACTS_FLUSH_SCHEDULED TRUE)
    cmake_language(DEFER DIRECTORY "${CMAKE_SOURCE_DIR}"
        CALL _nros_board_facts_flush)
endfunction()

function(_nros_board_facts_flush)
    get_property(_targets GLOBAL PROPERTY NROS_BOARD_FACTS_TARGETS)
    foreach(_t IN LISTS _targets)
        if(TARGET "${_t}")
            nros_board_facts_env("${_t}")
        endif()
    endforeach()
endfunction()

# nros_check_zephyr_rust_target()
#
# Issue 2204 — refuse a board descriptor whose `[board.zephyr] rust_target`
# disagrees with the triple zephyr-lang-rust compiles this Rust image for.
#
# The descriptor's triple is what `nros setup` installs and the preflight checks
# for; zephyr-lang-rust derives the real one from the Kconfig at build time, in
# `_rust_map_target`. Asking THAT function (not a copy of its table) is what
# makes this the triple the build uses. It disagreed silently on an RP2350: the
# descriptor said `thumbv8m.main-none-eabihf`, Zephyr built
# `thumbv8m.main-none-eabi` because `CONFIG_FP_HARDABI` was off.
#
# Deferred to the end of the top-level scope (zephyr/CMakeLists.txt): by then
# zephyr-lang-rust's module has defined `_rust_map_target` whatever the module
# order, and the image's `CONFIG_*` are in that scope. A C image (no
# `CONFIG_RUST`) and a configure that resolved no board facts make no claim.
function(nros_check_zephyr_rust_target)
    if(NOT CONFIG_RUST OR NOT COMMAND _rust_map_target)
        return()
    endif()
    get_property(_cmd GLOBAL PROPERTY NROS_BOARD_FACTS_RESOLVED_CMD)
    if("${_cmd}" STREQUAL "")
        return()
    endif()
    _rust_map_target()
    if("${RUST_TARGET}" STREQUAL "")
        return()
    endif()
    execute_process(
        COMMAND ${_cmd} --rust-target "${RUST_TARGET}"
        OUTPUT_VARIABLE _out
        ERROR_VARIABLE _err
        RESULT_VARIABLE _rc)
    if(NOT _rc EQUAL 0)
        message(FATAL_ERROR "nano-ros: ${_err}")
    endif()
endfunction()
