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
