# NanoRosCli.cmake -- `nros_resolve_cli`, the ONE lookup for the `nros` CLI
# (issues 0219 / 0325 / 1263).
#
# Split out of NanoRosCodegenCore.cmake by phase-481 W1: the Zephyr module's
# `module_ext_root` hook (`zephyr/modules/modules.cmake`) runs inside
# `find_package(Zephyr)`, before the application's `project()`, and needs the
# CLI to render the image's Kconfig fragment. It includes only this file;
# NanoRosCodegenCore.cmake includes it too, so there is still one function.

include_guard(GLOBAL)

# nros_resolve_cli(<out_var> [CONTEXT <caller-label>] [OPTIONAL])
#
# THE shared resolver for the `nros` CLI binary — issue #219 retired the four
# divergent hand-written copies (NanoRosEntry, nano_ros_workspace_metadata,
# zephyr nros_system_generate, and the find half of
# `_nros_resolve_codegen_tool` in NanoRosCodegenCore.cmake). One documented precedence order:
#
#   1. `$ENV{NROS_CLI}` — explicit per-invocation override (must EXIST).
#   2. An already-resolved shared codegen-tool cache var
#      (`_NANO_ROS_CODEGEN_TOOL`, then `_NROS_ZEPHYR_CODEGEN_TOOL`) when it
#      holds a real path — one configure resolves the CLI once.
#   3. `find_program`: the environment PATH first (activate.sh wires the
#      in-tree CLI — the sweep-contract SSoT), THEN the provisioned store
#      (`$NROS_HOME/bin`, `~/.nros/bin`) as PATHS fallbacks. PATHS, never
#      HINTS: hints are searched BEFORE PATH, so a stale provisioned
#      `~/.nros/bin/nros` would shadow the in-tree CLI (the museum-CLI trap
#      the zephyr resolver's comment documented — now enforced everywhere).
#
# FATAL when absent unless OPTIONAL (then <out_var> = NOTFOUND). The
# find_program result is cached (`_NROS_CLI_RESOLVED`); a cached path that no
# longer exists is dropped and re-detected.

function(nros_resolve_cli _out)
    cmake_parse_arguments(_RC "OPTIONAL" "CONTEXT" "" ${ARGN})
    if(NOT _RC_CONTEXT)
        set(_RC_CONTEXT "nano-ros")
    endif()
    if(DEFINED ENV{NROS_CLI} AND EXISTS "$ENV{NROS_CLI}")
        set(${_out} "$ENV{NROS_CLI}" PARENT_SCOPE)
        return()
    endif()
    foreach(_cv _NANO_ROS_CODEGEN_TOOL _NROS_ZEPHYR_CODEGEN_TOOL)
        if(DEFINED CACHE{${_cv}} AND ${_cv}
           AND NOT "${${_cv}}" MATCHES "^\\$<" AND EXISTS "${${_cv}}")
            set(${_out} "${${_cv}}" PARENT_SCOPE)
            return()
        endif()
    endforeach()
    if(_NROS_CLI_RESOLVED AND NOT EXISTS "${_NROS_CLI_RESOLVED}")
        message(STATUS "Cached nros CLI no longer exists: ${_NROS_CLI_RESOLVED}; re-detecting")
        unset(_NROS_CLI_RESOLVED CACHE)
    endif()
    set(_paths "$ENV{HOME}/.nros/bin")
    if(DEFINED ENV{NROS_HOME})
        list(PREPEND _paths "$ENV{NROS_HOME}/bin")
    endif()
    find_program(_NROS_CLI_RESOLVED NAMES nros PATHS ${_paths}
        DOC "nros CLI (shared resolver — issue #219)")
    if(_NROS_CLI_RESOLVED)
        set(${_out} "${_NROS_CLI_RESOLVED}" PARENT_SCOPE)
        return()
    endif()
    if(_RC_OPTIONAL)
        set(${_out} "NOTFOUND" PARENT_SCOPE)
        return()
    endif()
    message(FATAL_ERROR
        "${_RC_CONTEXT}: `nros` CLI not found on PATH or in the provisioned "
        "store. nano-ros builds it in-tree from packages/cli/ (Phase 218):\n"
        "  ./scripts/bootstrap.sh && source ./activate.sh   (contributors: just setup-cli)\n"
        "or set $NROS_CLI to an explicit binary.")
endfunction()

# nros_codegen_tool_reconfigure(<tool>)
#
# issue 1018 — make a CONFIGURE-TIME emitter re-run when the tool that emits is
# rebuilt.
#
# The build-time generator states the edge outright: its codegen
# `add_custom_command` carries `DEPENDS … "${_NANO_ROS_CODEGEN_TOOL}"`, so a
# newer `nros` re-emits (and CMake's `restat = 1` plus codegen's
# write-if-changed keep an emitter-identical rebuild from cascading downstream —
# measured, the command re-runs once and nothing else rebuilds).
#
# A configure-time emitter has no such edge available: `execute_process()` runs
# during configure, so its freshness is decided by whether a configure happens
# AT ALL. The Zephyr interfaces generator even carries the right predicate
# already — an `IS_NEWER_THAN` loop that names the tool — but it is dead on an
# incremental build, because nothing makes `build.ninja` stale when the tool
# moves. Measured in this checkout: `zephyr-workspace/build-rust-talker-zenoh`'s
# RERUN_CMAKE edge lists 3592 inputs and NOT ONE is under `packages/cli`.
#
# `nano_ros_entry()` learned this as issue #182 and registered the binary right
# there, in its own function. That fixed the site, not the class: the three
# sibling configure-time emitters (this Zephyr interfaces lane,
# `nros_system_generate()`, the ESP-IDF shim's `codegen-system`) kept nothing,
# and a Zephyr image inherits freshness only if it ALSO happens to call
# `nano_ros_entry()` — which no single-example image does. So an image that
# generates its interfaces at configure time and does not use an entry package
# keeps museum generated code after a `nros` rebuild, silently.
#
# WHY THE BINARY AND NOT `nros codegen-fingerprint`
#
# The fingerprint (RFC-0061 / phase-318 W1) is the better key where it applies:
# it hashes what the EMITTERS produce for a compiled-in corpus, so 41 distinct
# `nros` binaries map to 9 fingerprints and 78 % of `just setup-cli` rebuilds
# would cost nothing. But its corpus covers the MESSAGE/SERVICE/ACTION emitters
# only — not `codegen entry`, not `codegen-system` — so keying those two on it
# would report FRESH for a real change to their emitters. One key for all four
# sites, and it is the conservative one. Narrowing the interfaces site onto the
# fingerprint is a follow-up that needs a producer for the fingerprint value
# outside the build dir (a configure cannot write its own configure input
# without a first configure to write it).
#
# The widening this costs was measured before it landed: of the 7 Zephyr build
# dirs in this checkout, 3 reach a configure-time emitter and all 3 already
# carry the CLI as a configure dependency via `nano_ros_entry()`; the other 4
# reach no configure-time emitter and gain nothing, because this registers at
# the emitter's own call site. Zero build dirs gain a reconfigure. What changes
# is that the freshness stops being an accident of which other function the
# image happened to call.
#
# Deduplicated because a single directory legitimately reaches several emitters.
function(nros_codegen_tool_reconfigure _tool)
    if(_tool STREQUAL "" OR NOT EXISTS "${_tool}")
        return()
    endif()
    get_property(_nctr_deps DIRECTORY PROPERTY CMAKE_CONFIGURE_DEPENDS)
    if(NOT "${_tool}" IN_LIST _nctr_deps)
        set_property(DIRECTORY APPEND PROPERTY CMAKE_CONFIGURE_DEPENDS "${_tool}")
    endif()
endfunction()
