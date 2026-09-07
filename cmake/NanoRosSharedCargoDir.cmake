# NanoRosSharedCargoDir.cmake — the ONE keyed shared-cargo-directory rule.
#
# phase-400 W5.b — split out of NanoRosCorrosion.cmake, unchanged.
#
# It lived there because its first two consumers did (`nros_share_corrosion_cargo_dir`
# and the NuttX FFI driver). The Zephyr C/C++ lane is the third, and it uses no
# Corrosion at all — `zephyr/CMakeLists.txt` never includes that module, so the
# helper was simply unreachable from the platform with 89 unshared cargo
# directories. Including NanoRosCorrosion.cmake to reach it would drag Corrosion
# provisioning into a lane that has no use for it.
#
# The alternative — a second normalise-and-hash in the Zephyr module — is the
# thing the doc block below explicitly forbids, and the file it came from records
# what an unstable key costs. So the rule moves to its own file and every
# consumer includes THIS.

include_guard(GLOBAL)

# =============================================================================
# The KNOB half of every cargo-directory key — RFC-0094 D4, phase-439 W1.
#
# `nros_shared_cargo_dir()` below hashes the fields a caller hands it, and until
# this existed the `nros-c` and NuttX callers handed it `features, rmw, board,
# caps, profile, target` and **no knob values**. The Zephyr lane already keyed on
# every `NROS_RESOLVED_*` (issue 0528 measured what omitting them cost there: two
# leaves at the same (target, features) disagreeing on
# `CONFIG_NROS_EXECUTOR_MAX_CBS` shared a probe dir, and one compiled against a
# constant sized for the other).
#
# Why that gap is not a follow-up: cargo uplifts the final archive to an UNHASHED
# name (`libnros_c.a`), so two configurations sharing a directory overwrite each
# other's artifact and one silently links the other's numbers — issue 0616's
# shape. RFC-0094's resolve phase makes per-image knob divergence the NORMAL case
# rather than the exception, so landing it without this would be worse than the
# status quo.
#
# ONE spelling, for the reason this file exists at all: a second per-caller list
# is how the two would drift (issue 1025 — the fixture group key had one FORMULA
# and two derivations of its INPUTS, and no esp32 flash image could be packed).
#
# TWO ROADS, because the tree has two knob-delivery mechanisms and they do not
# overlap in time:
#
#  1. the RESOLVER registry — `NROS_RESOLVED_KNOBS` + `NROS_RESOLVED_<name>`,
#     filled by `nros_resolve_knobs()`. Zephyr today; every lane once RFC-0094's
#     stage 3.5 writes `resolved.toml` and this function reads it.
#  2. the ENVIRONMENT — the road that reaches a build script on EVERY lane and
#     is the one knob value knowable this early on a lane with no resolver.
#     It is not a guess: `_nros_resolve_knob()`'s rung 1 is `$ENV{<name>}`, and
#     `examples/fixtures.toml` states knobs exactly this way
#     (`env = { ZPICO_MAX_QUERYABLES = "2" }`).
#
# Road 1 wins per name. On Zephyr an env-stated knob has ALREADY been registered
# by rung 1 of `_nros_resolve_knob()`, so road 2 contributes nothing and the key
# text is what that lane produced before this existed. (Road 2 can still add a
# name on Zephyr in one case: an env-stated knob whose `_nros_resolve_knob()`
# call the configure never reached — a backend-specific knob on the other
# backend, say. That is a knob the reading build script WOULD see, so separating
# on it is the answer, not a regression.) On a lane with no knob stated at all
# BOTH roads are empty, the field list is empty, and the key text is unchanged:
# a warm cargo directory is not invalidated for nothing. Measured — the `nros-c`
# lane's real configure resolves to the same `d20f4167871d` before and after
# this change when no knob is stated, and to a different directory when one is.
# =============================================================================

# The knob NAME inventory, harvested from its ONE authored site.
#
# `_nros_resolve_knob(<NAME> ...)` / `_nros_resolve_derivable_knob(<NAME> ...)`
# in `zephyr/cmake/nros_cargo_build.cmake` is where this tree writes down which
# environment variables are knobs. `check-kconfig-knob-forwarding` already
# treats that set as authoritative — it holds every name there to a Rust build
# script that reads it — so those build scripts run on every lane and an
# env-stated value of any of these names changes compiled output on any lane.
#
# READ rather than copied. A cmake-side list of the same 43 names would be a
# second spelling of an authored set, which is the drift this repo keeps paying
# for; nano-ros is consumed only as a source distribution (`add_subdirectory`),
# so the file is always present and its absence is a real breakage rather than a
# reason to degrade.
set(_NROS_KNOB_INVENTORY_FILE
    "${CMAKE_CURRENT_LIST_DIR}/../zephyr/cmake/nros_cargo_build.cmake"
    CACHE INTERNAL "phase-439 W1 — the authored knob-name inventory")

function(_nros_knob_inventory out_var)
    if(DEFINED NROS_KNOB_INVENTORY_CACHED)
        set(${out_var} "${NROS_KNOB_INVENTORY_CACHED}" PARENT_SCOPE)
        return()
    endif()
    if(NOT EXISTS "${_NROS_KNOB_INVENTORY_FILE}")
        message(FATAL_ERROR
            "nano-ros: the knob inventory is missing — "
            "${_NROS_KNOB_INVENTORY_FILE} does not exist. Every shared cargo "
            "directory keys on the knob values (RFC-0094 D4), and an EMPTY "
            "inventory would key on none of them silently, which is the "
            "collision this exists to prevent. Do not degrade past this.")
    endif()
    file(STRINGS "${_NROS_KNOB_INVENTORY_FILE}" _lines
        REGEX "_nros_resolve(_derivable)?_knob\\([A-Z0-9_]+")
    set(_names "")
    foreach(_line IN LISTS _lines)
        string(REGEX MATCHALL "_nros_resolve(_derivable)?_knob\\([A-Z0-9_]+"
            _hits "${_line}")
        foreach(_hit IN LISTS _hits)
            string(REGEX REPLACE "^_nros_resolve(_derivable)?_knob\\(" "" _n "${_hit}")
            list(APPEND _names "${_n}")
        endforeach()
    endforeach()
    list(REMOVE_DUPLICATES _names)
    list(SORT _names)
    if(NOT _names)
        message(FATAL_ERROR
            "nano-ros: the knob inventory at ${_NROS_KNOB_INVENTORY_FILE} "
            "yielded NO names. Either the resolver's call spelling changed or "
            "the file did; an empty inventory silently keys every shared cargo "
            "directory on zero knobs (RFC-0094 D4). Fix the harvest, do not "
            "let it pass empty.")
    endif()
    set(NROS_KNOB_INVENTORY_CACHED "${_names}" CACHE INTERNAL
        "phase-439 W1 — knob names harvested from the resolver's own call sites")
    set(${out_var} "${_names}" PARENT_SCOPE)
endfunction()

# nros_knob_key_fields(<out_var>)
#
# The knob fields for a cargo-directory KEY, as `<NAME>=<value>` elements ready
# to append to `nros_shared_cargo_dir(... KEY ...)`. Empty when this configure
# has resolved and states no knob — which is the whole reason an existing image
# whose knobs did not move keeps the directory it already has.
function(nros_knob_key_fields out_var)
    set(_fields "")
    set(_seen "")
    # Road 1 — the resolver registry, in registry order, so a lane that already
    # keyed on it produces the same text it always did.
    foreach(_k IN LISTS NROS_RESOLVED_KNOBS)
        if(NOT "${_k}" IN_LIST _seen)
            list(APPEND _seen "${_k}")
            list(APPEND _fields "${_k}=${NROS_RESOLVED_${_k}}")
        endif()
    endforeach()
    # Road 2 — the environment, for the names the resolver did not answer for.
    _nros_knob_inventory(_names)
    foreach(_k IN LISTS _names)
        if("${_k}" IN_LIST _seen)
            continue()
        endif()
        if(DEFINED ENV{${_k}} AND NOT "$ENV{${_k}}" STREQUAL "")
            list(APPEND _seen "${_k}")
            list(APPEND _fields "${_k}=$ENV{${_k}}")
        endif()
    endforeach()
    set(${out_var} "${_fields}" PARENT_SCOPE)
endfunction()

# nros_shared_cargo_dir(<out_var> KEY <field>...)
#
# Issue 0805 — resolve `NROS_SHARED_CARGO_ROOT` + a KEY to a concrete shared
# cargo directory, creating it and recording the key text beside it. Returns
# empty in `<out_var>` when sharing was not requested.
#
# Factored out because there are TWO consumers with different plumbing and only
# one keying rule is allowed to exist:
#
#  * `nros_share_corrosion_cargo_dir()` below — Corrosion computes its own
#    `--target-dir` from `CMAKE_BINARY_DIR`, so that path has to be redirected
#    with a symlink.
#  * the NuttX FFI driver (`packages/api/nros-c/cmake/nros-nuttx.cmake`) — a
#    hand-rolled cargo invocation that sets `CARGO_TARGET_DIR` itself, so it
#    takes this path directly and needs no symlink at all.
#
# A second copy of the normalise-and-hash rule is how the two would drift apart,
# and this file already records what an unstable key costs.
function(nros_shared_cargo_dir out_var)
    set(${out_var} "" PARENT_SCOPE)
    if(DEFINED CACHE{NROS_SHARED_CARGO_ROOT} AND NROS_SHARED_CARGO_ROOT STREQUAL "")
        message(FATAL_ERROR
            "nano-ros: -DNROS_SHARED_CARGO_ROOT was passed EMPTY. The caller's "
            "path variable did not expand (issue 0805). Fix the caller; an "
            "empty value would silently fall back to per-leaf cargo dirs.")
    endif()
    if(NOT NROS_SHARED_CARGO_ROOT)
        return()
    endif()
    cmake_parse_arguments(_SD "" "" "KEY" ${ARGN})
    if(NOT _SD_KEY)
        message(FATAL_ERROR
            "nros_shared_cargo_dir: KEY is required. Sharing a cargo directory "
            "between two configurations that differ is the defect this exists "
            "to avoid, so there is no default key.")
    endif()
    # Normalise before hashing — see the long note on the wrapper below: the
    # key must be a function of the CONFIGURATION, not of how its lists were
    # assembled.
    set(_key_norm "")
    foreach(_field IN LISTS _SD_KEY)
        if(_field MATCHES "^([^=]+)=(.*)$")
            set(_fname "${CMAKE_MATCH_1}")
            set(_fval "${CMAKE_MATCH_2}")
            if(NOT _fval STREQUAL "")
                string(REPLACE "," ";" _fparts "${_fval}")
                list(REMOVE_DUPLICATES _fparts)
                list(SORT _fparts)
                string(REPLACE ";" "," _fval "${_fparts}")
            endif()
            list(APPEND _key_norm "${_fname}=${_fval}")
        else()
            list(APPEND _key_norm "${_field}")
        endif()
    endforeach()
    string(REPLACE ";" "|" _key_text "${_key_norm}")
    string(SHA1 _key_hash "${_key_text}")
    string(SUBSTRING "${_key_hash}" 0 12 _key_hash)
    set(_dir "${NROS_SHARED_CARGO_ROOT}/${_key_hash}")
    file(MAKE_DIRECTORY "${_dir}")
    file(WRITE "${_dir}.key" "${_key_text}\n")
    set(${out_var} "${_dir}" PARENT_SCOPE)
    set(${out_var}_KEY_TEXT "${_key_text}" PARENT_SCOPE)
endfunction()
